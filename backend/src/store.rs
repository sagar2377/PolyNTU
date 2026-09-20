use crate::{
    amm, auth,
    cache::{AuthAccount, Cache},
    error::{Error, Result, conflict, invalid},
    events,
    market::{Instance, NewInstance, NewSeries, Series, inside_active_window, sgt_iso_day},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, PgConnection, PgPool, Row, postgres::PgPoolOptions, types::Json};
use std::sync::{Arc, LazyLock};
use uuid::Uuid;

#[derive(Clone)]
pub struct Store {
    pub pool: PgPool,
    pub demo_mode: bool,
    pub cache: Cache,
    /// The NTU Bus API provider the bus adapter asks before falling back to
    /// the simulated feed; the entry point sets it from
    /// POLYNTU_NTUBUS_PROVIDER (empty disables the live path).
    pub ntubus_provider: Option<String>,
    /// Serializes settlement passes: the worker loop and the on-demand
    /// `/admin/clock/advance` and `/admin/worker/tick` endpoints all run
    /// `worker::tick`, and concurrent passes would select the same due
    /// instances.
    pub tick_lock: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub display_name: String,
    pub email: Option<String>,
    pub role: Option<String>,
    pub balance_micros: String,
}

/// Issuance policy in whole units (ADR 0005). The original 1M-unit budget
/// funded one hundred welcome gifts; total issuance rises to 1B so it does
/// not run out. The raise is appended by `initialize`, not a migration,
/// because migrations run before the issuance account exists on a fresh
/// database.
pub const INITIAL_ISSUANCE_UNITS: i64 = 1_000_000;
pub const TOTAL_ISSUANCE_UNITS: i64 = 1_000_000_000;

/// Terminal recurring-series brackets are purged once their evidence
/// deadline is this far in the past; one-time markets are kept indefinitely.
pub const BRACKET_RETENTION_MS: i64 = 86_400_000;
/// The treasury-funded welcome gift for a registered account.
pub const WELCOME_GIFT_UNITS: i64 = 10_000;
/// The one-click demo account keeps its smaller development grant.
const DEMO_GRANT_UNITS: i64 = 1_000;

fn valid_display_name(name: &str) -> bool {
    name.len() >= 2 && name.len() <= 60 && !name.chars().any(char::is_control)
}

/// Unknown-email logins verify against this fixed hash so they cost the
/// same argon2 work as wrong-password logins; accounts cannot be
/// enumerated through response time.
static LOGIN_TIMING_HASH: LazyLock<String> = LazyLock::new(|| {
    auth::hash_password("polyntu-login-timing-equalizer").expect("fixed password")
});

pub async fn db_now(connection: &mut PgConnection) -> Result<i64> {
    Ok(sqlx::query_scalar("SELECT (extract(epoch FROM clock_timestamp()) * 1000)::BIGINT + clock_offset_ms FROM settings WHERE singleton")
        .fetch_one(connection).await?)
}

pub async fn lock_accounts(connection: &mut PgConnection, ids: &[String]) -> Result<()> {
    // Balance updates do not change account keys. Allow concurrent FK KEY SHARE
    // locks from idempotency inserts; FOR UPDATE would cause lock-upgrade cycles.
    sqlx::query("SELECT id FROM accounts WHERE id = ANY($1) ORDER BY id FOR NO KEY UPDATE")
        .bind(ids)
        .fetch_all(connection)
        .await?;
    Ok(())
}

pub async fn balance(connection: &mut PgConnection, account: &str) -> Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT balance_micros FROM accounts WHERE id=$1")
            .bind(account)
            .fetch_one(connection)
            .await?,
    )
}

pub async fn transfer(
    connection: &mut PgConnection,
    from: &str,
    to: &str,
    amount: i64,
    kind: &str,
    reference: &str,
    now: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO ledger_transfers(id,from_account,to_account,amount_micros,kind,reference,created_ms) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(Uuid::new_v4().to_string()).bind(from).bind(to).bind(amount).bind(kind).bind(reference).bind(now)
        .execute(connection).await?;
    Ok(())
}

pub async fn event(
    connection: &mut PgConnection,
    instance: &str,
    version: i64,
    kind: &str,
    now: i64,
) -> Result<()> {
    // One statement: the durable outbox row plus the commit-time notification
    // that fans the event out to listening processes.
    sqlx::query("WITH o AS (INSERT INTO outbox(instance_id,instance_version,event_type,created_ms) VALUES($1,$2,$3,$4) RETURNING id) SELECT pg_notify($5, json_build_object('id',o.id,'instance_id',$1,'version',$2,'type',$3,'created_ms',$4)::text) FROM o")
        .bind(instance).bind(version).bind(kind).bind(now).bind(events::CHANNEL)
        .execute(connection).await?;
    Ok(())
}

pub async fn audit(
    connection: &mut PgConnection,
    action: &str,
    instance: Option<&str>,
    detail: Value,
    now: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO admin_audit(action,instance_id,detail,created_ms) VALUES($1,$2,$3,$4)",
    )
    .bind(action)
    .bind(instance)
    .bind(Json(detail))
    .bind(now)
    .execute(connection)
    .await?;
    Ok(())
}

pub fn instance_view(instance: &Instance, now: i64) -> Result<Value> {
    let prices = amm::prices(&instance.inventory, instance.liquidity_units)?;
    let outcomes: Vec<Value> = instance
        .outcomes
        .iter()
        .enumerate()
        .map(|(index, outcome)| {
            json!({
                "id": outcome.id, "label": outcome.label, "probability": prices[index],
                "outstanding_millis": instance.inventory[index]
            })
        })
        .collect();
    Ok(json!({
        "id": instance.id, "template_id": instance.template_id, "category": instance.category,
        "title": instance.title, "resolution_criterion": instance.resolution_criterion,
        "rule": instance.rule.0, "source_id": instance.source_id, "data_mode": instance.data_mode,
        "state": if instance.state == "open" && now >= instance.close_ms { "closed" } else { &instance.state },
        "suspended": instance.suspended, "tradable": instance.tradable(now), "outcomes": outcomes,
        "version": instance.version, "close_ms": instance.close_ms,
        "observation_start_ms": instance.observation_start_ms, "observation_end_ms": instance.observation_end_ms,
        "finalize_after_ms": instance.finalize_after_ms, "evidence_deadline_ms": instance.evidence_deadline_ms,
        "liquidity_units": instance.liquidity_units, "result": instance.result, "fee_charged": instance.fee_charged,
        "creator_account_id": instance.creator_account_id,
        "series_id": instance.series_id, "bracket_start_ms": instance.bracket_start_ms,
        "evidence_id": instance.evidence_id, "server_time_ms": now,
        "void_policy": "Each outcome share redeems for 1/n units; aggregate account credits round down to a micro-unit."
    }))
}

impl Store {
    pub async fn connect(url: &str, demo_mode: bool) -> Result<Self> {
        let pool = PgPoolOptions::new().max_connections(32)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .after_connect(|conn, _| Box::pin(async move {
                sqlx::query("SELECT set_config('lock_timeout','2s',false), set_config('statement_timeout','15s',false)")
                    .execute(conn).await?;
                Ok(())
            })).connect(url).await?;
        sqlx::migrate!()
            .run(&pool)
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        let store = Self {
            pool,
            demo_mode,
            cache: Cache::new(0),
            ntubus_provider: None,
            tick_lock: Arc::new(tokio::sync::Mutex::new(())),
        };
        let clock_offset = store.initialize().await?;
        store.cache.set_clock_offset(clock_offset);
        Ok(store)
    }

    async fn initialize(&self) -> Result<i64> {
        let mut tx = self.pool.begin().await?;
        // Serialize bootstrap across API/worker processes.
        sqlx::query("SELECT singleton FROM settings WHERE singleton FOR UPDATE")
            .execute(&mut *tx)
            .await?;
        let offset: i64 =
            sqlx::query_scalar("SELECT clock_offset_ms FROM settings WHERE singleton")
                .fetch_one(&mut *tx)
                .await?;
        if !self.demo_mode && offset != 0 {
            return Err(invalid(
                "Use a separate database for non-demo mode; this database has a simulated clock offset",
            ));
        }
        let initialized: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts WHERE id='issuance')")
                .fetch_one(&mut *tx)
                .await?;
        let stored_mode: bool =
            sqlx::query_scalar("SELECT demo_mode FROM settings WHERE singleton")
                .fetch_one(&mut *tx)
                .await?;
        if initialized && stored_mode != self.demo_mode {
            return Err(invalid(
                "Demo mode is fixed at database initialization; use a separate database to change modes",
            ));
        }
        sqlx::query("UPDATE settings SET demo_mode=$1,simulation_secret=COALESCE(simulation_secret,$2) WHERE singleton")
            .bind(self.demo_mode)
            .bind(auth::random_token()?)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO accounts(id,display_name,kind) VALUES('issuance','Unit issuance','issuance'),('treasury','Platform subsidy budget','treasury') ON CONFLICT DO NOTHING")
            .execute(&mut *tx).await?;
        let now = db_now(&mut tx).await?;
        sqlx::query("INSERT INTO ledger_transfers(id,from_account,to_account,amount_micros,kind,reference,created_ms) VALUES('initial-issuance','issuance','treasury',$1,'issuance','initial-issuance',$2) ON CONFLICT DO NOTHING")
            .bind(INITIAL_ISSUANCE_UNITS * amm::CREDIT_SCALE).bind(now).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO ledger_transfers(id,from_account,to_account,amount_micros,kind,reference,created_ms) VALUES('issuance-expansion','issuance','treasury',$1,'issuance','issuance-expansion',$2) ON CONFLICT DO NOTHING")
            .bind((TOTAL_ISSUANCE_UNITS - INITIAL_ISSUANCE_UNITS) * amm::CREDIT_SCALE).bind(now).execute(&mut *tx).await?;
        // Demo and test databases seed an administrator account:
        // admin@ntu.edu.sg with password "admin". Its session token is
        // generated fresh at each login; the seed token's plaintext is
        // discarded, so the only way in is the password.
        if self.demo_mode {
            let seeded_token = auth::random_token()?;
            sqlx::query(
                "INSERT INTO accounts(id,display_name,kind,token_hash,email,password_hash,role) VALUES('demo-admin','Administrator','user',$1,'admin@ntu.edu.sg',$2,'admin') ON CONFLICT (email) DO NOTHING",
            )
            .bind(auth::hash(seeded_token.as_bytes()))
            .bind(auth::hash_password("admin")?)
            .execute(&mut *tx)
            .await?;
            // The seeded administrator trades in the demo too, so it gets
            // the same treasury-funded welcome gift, exactly once.
            let granted: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM ledger_transfers WHERE reference='grant:demo-admin')",
            )
            .fetch_one(&mut *tx)
            .await?;
            if !granted {
                lock_accounts(&mut tx, &["demo-admin".into(), "treasury".into()]).await?;
                transfer(
                    &mut *tx,
                    "treasury",
                    "demo-admin",
                    WELCOME_GIFT_UNITS * amm::CREDIT_SCALE,
                    "grant",
                    "grant:demo-admin",
                    now,
                )
                .await?;
            }
        }
        tx.commit().await?;
        Ok(offset)
    }

    pub async fn now(&self) -> Result<i64> {
        db_now(&mut *self.pool.acquire().await?).await
    }

    /// Local estimate of the database clock for preview-only responses.
    pub fn now_cached(&self) -> i64 {
        self.cache.now_ms()
    }

    pub async fn account_for_token(&self, token: &str) -> Result<Account> {
        if token.len() > 200 {
            return Err(Error::Unauthorized);
        }
        sqlx::query_as("SELECT id,display_name,email,role,balance_micros::TEXT FROM accounts WHERE token_hash=$1 AND kind='user'")
            .bind(auth::hash(token.as_bytes())).fetch_optional(&self.pool).await?.ok_or(Error::Unauthorized)
    }

    /// Cached token resolution for the authenticated hot paths. Token-to-
    /// account mapping is immutable, so entries never go stale; balances are
    /// always read separately when they matter.
    pub async fn auth_account_for_token(&self, token: &str) -> Result<Arc<AuthAccount>> {
        if token.len() > 200 {
            return Err(Error::Unauthorized);
        }
        let token_hash = auth::hash(token.as_bytes());
        if let Some(cached) = self.cache.account(&token_hash) {
            return Ok(cached);
        }
        let record =
            sqlx::query("SELECT id,display_name FROM accounts WHERE token_hash=$1 AND kind='user'")
                .bind(&token_hash)
                .fetch_optional(&self.pool)
                .await?
                .ok_or(Error::Unauthorized)?;
        let account = AuthAccount {
            id: record.get("id"),
            display_name: record.get("display_name"),
        };
        Ok(self.cache.put_account(token_hash, account))
    }

    pub async fn account_by_id(&self, id: &str) -> Result<Account> {
        sqlx::query_as(
            "SELECT id,display_name,email,role,balance_micros::TEXT FROM accounts WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Error::NotFound)
    }

    /// Whether the account holds the admin role. Read fresh on every admin
    /// request: role changes are rare and the auth cache carries no roles.
    pub async fn account_is_admin(&self, id: &str) -> Result<bool> {
        let admin: Option<bool> =
            sqlx::query_scalar("SELECT role='admin' FROM accounts WHERE id=$1 AND kind='user'")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(admin.unwrap_or(false))
    }

    /// Read-through instance lookup for the quote path. A miss fetches the
    /// row and caches it; mutations invalidate or write through before
    /// returning, so a hit is at most one trade behind.
    pub async fn instance_cached(&self, id: &str) -> Result<Arc<Instance>> {
        if let Some(hit) = self.cache.instance(id) {
            return Ok(hit);
        }
        let instance = self.instance(id).await?;
        Ok(self.cache.put_instance(instance))
    }

    pub async fn create_account(&self, display_name: &str) -> Result<Value> {
        let name = display_name.trim();
        if !valid_display_name(name) {
            return Err(invalid(
                "Display name must be 2–60 characters without control characters",
            ));
        }
        let id = Uuid::new_v4().to_string();
        let token = auth::random_token()?;
        let grant = DEMO_GRANT_UNITS * amm::CREDIT_SCALE;
        let mut tx = self.pool.begin().await?;
        let now = db_now(&mut tx).await?;
        sqlx::query(
            "INSERT INTO accounts(id,display_name,kind,token_hash) VALUES($1,$2,'user',$3)",
        )
        .bind(&id)
        .bind(name)
        .bind(auth::hash(token.as_bytes()))
        .execute(&mut *tx)
        .await?;
        lock_accounts(&mut tx, &[id.clone(), "treasury".into()]).await?;
        if balance(&mut tx, "treasury").await? < grant {
            return Err(conflict("The platform's grant budget is exhausted"));
        }
        transfer(
            &mut tx,
            "treasury",
            &id,
            grant,
            "grant",
            &format!("grant:{id}"),
            now,
        )
        .await?;
        tx.commit().await?;
        Ok(
            json!({"token": token, "account": {"id": id, "display_name": name, "email": null, "role": null, "balance_micros": grant.to_string()}}),
        )
    }

    /// Register an account with a unique NTU email and a password (ADR 0005).
    /// The plaintext session token is returned once; only its SHA-256 hash is
    /// stored, exactly like the demo account token.
    pub async fn register_account(
        &self,
        display_name: &str,
        email: &str,
        password: &str,
    ) -> Result<Value> {
        let name = display_name.trim();
        if !valid_display_name(name) {
            return Err(invalid(
                "Display name must be 2–60 characters without control characters",
            ));
        }
        let email = email.trim().to_lowercase();
        if !auth::valid_ntu_email(&email) {
            return Err(invalid(
                "Register with an NTU email (name@ntu.edu.sg or name@unit.ntu.edu.sg)",
            ));
        }
        if password.chars().count() < 12 {
            return Err(invalid("Password must be at least 12 characters"));
        }
        let id = Uuid::new_v4().to_string();
        let token = auth::random_token()?;
        let grant = WELCOME_GIFT_UNITS * amm::CREDIT_SCALE;
        let mut tx = self.pool.begin().await?;
        let now = db_now(&mut tx).await?;
        let inserted = sqlx::query(
            "INSERT INTO accounts(id,display_name,kind,token_hash,email,password_hash,role) VALUES($1,$2,'user',$3,$4,$5,'member') ON CONFLICT (email) DO NOTHING RETURNING id",
        )
        .bind(&id)
        .bind(name)
        .bind(auth::hash(token.as_bytes()))
        .bind(&email)
        .bind(auth::hash_password(password)?)
        .fetch_optional(&mut *tx)
        .await?;
        if inserted.is_none() {
            return Err(conflict("This NTU email is already registered"));
        }
        lock_accounts(&mut tx, &[id.clone(), "treasury".into()]).await?;
        if balance(&mut tx, "treasury").await? < grant {
            return Err(conflict("The platform's grant budget is exhausted"));
        }
        transfer(
            &mut tx,
            "treasury",
            &id,
            grant,
            "grant",
            &format!("grant:{id}"),
            now,
        )
        .await?;
        tx.commit().await?;
        Ok(
            json!({"token": token, "account": {"id": id, "display_name": name, "email": email, "role": "member", "balance_micros": grant.to_string()}}),
        )
    }

    /// Password login (ADR 0005). A successful login rotates the account's
    /// bearer token: the new plaintext is returned once and the previous
    /// token stops working immediately in this process.
    pub async fn login(&self, email: &str, password: &str) -> Result<Value> {
        let email = email.trim().to_lowercase();
        let row = sqlx::query(
            "SELECT id,display_name,password_hash,token_hash FROM accounts WHERE email=$1 AND kind='user'",
        )
        .bind(&email)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            let _ = auth::verify_password(&LOGIN_TIMING_HASH, password);
            return Err(Error::InvalidCredentials);
        };
        let stored: String = row.get("password_hash");
        if !auth::verify_password(&stored, password) {
            return Err(Error::InvalidCredentials);
        }
        let id: String = row.get("id");
        let previous_hash: String = row.get("token_hash");
        let token = auth::random_token()?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE accounts SET token_hash=$1 WHERE id=$2")
            .bind(auth::hash(token.as_bytes()))
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.cache.evict_account(&previous_hash);
        let account = self.account_by_id(&id).await?;
        Ok(json!({"token": token, "account": account}))
    }

    /// A member files a creator verification request (ADR 0005). One pending
    /// request per account; a rejected member may apply again.
    pub async fn create_verification_request(&self, account: &str) -> Result<Value> {
        let mut tx = self.pool.begin().await?;
        let now = db_now(&mut tx).await?;
        // Lock the account row so an approval cannot interleave with this
        // request; the role check below then cannot go stale.
        let record =
            sqlx::query("SELECT role FROM accounts WHERE id=$1 AND kind='user' FOR NO KEY UPDATE")
                .bind(account)
                .fetch_optional(&mut *tx)
                .await?;
        let role: Option<String> = match &record {
            Some(row) => row.get("role"),
            None => return Err(Error::NotFound),
        };
        match role.as_deref() {
            Some("creator") => return Err(invalid("This account is already a creator")),
            Some("member") => {}
            Some("admin") => {
                return Err(invalid(
                    "Administrators do not request creator verification",
                ));
            }
            _ => {
                return Err(invalid(
                    "Register with an NTU email before requesting verification",
                ));
            }
        }
        let id = Uuid::new_v4().to_string();
        let inserted = sqlx::query(
            "INSERT INTO verification_requests(id,account_id,status,created_ms) VALUES($1,$2,'pending',$3) ON CONFLICT DO NOTHING RETURNING id",
        )
        .bind(&id)
        .bind(account)
        .bind(now)
        .fetch_optional(&mut *tx)
        .await?;
        if inserted.is_none() {
            return Err(conflict("A pending verification request already exists"));
        }
        tx.commit().await?;
        Ok(json!({"id": id, "account_id": account, "status": "pending", "created_ms": now}))
    }

    /// The account's latest verification request, if any, for the requester's
    /// own interface.
    pub async fn verification_request(&self, account: &str) -> Result<Option<Value>> {
        let row = sqlx::query(
            "SELECT id,status,reason,created_ms,decided_ms FROM verification_requests WHERE account_id=$1 ORDER BY created_ms DESC,id LIMIT 1",
        )
        .bind(account)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| {
            json!({"id": r.get::<String,_>("id"), "status": r.get::<String,_>("status"),
                "reason": r.get::<Option<String>,_>("reason"), "created_ms": r.get::<i64,_>("created_ms"),
                "decided_ms": r.get::<Option<i64>,_>("decided_ms")})
        }))
    }

    /// The administrator's review list, newest first.
    pub async fn verification_requests(&self, status: Option<&str>) -> Result<Vec<Value>> {
        if status.is_some_and(|status| !matches!(status, "pending" | "approved" | "rejected")) {
            return Err(invalid("Status must be pending, approved, or rejected"));
        }
        let rows = sqlx::query(
            "SELECT v.id,v.account_id,v.status,v.reason,v.created_ms,v.decided_ms,a.display_name,a.email FROM verification_requests v JOIN accounts a ON a.id=v.account_id WHERE ($1::TEXT IS NULL OR v.status=$1) ORDER BY v.created_ms DESC,v.id LIMIT 200",
        )
        .bind(status)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| {
                json!({"id": r.get::<String,_>("id"), "account_id": r.get::<String,_>("account_id"),
                    "status": r.get::<String,_>("status"), "reason": r.get::<Option<String>,_>("reason"),
                    "created_ms": r.get::<i64,_>("created_ms"), "decided_ms": r.get::<Option<i64>,_>("decided_ms"),
                    "display_name": r.get::<String,_>("display_name"), "email": r.get::<Option<String>,_>("email")})
            })
            .collect())
    }

    /// The administrator approves or rejects a pending request. Approval
    /// permanently grants the creator role; rejection records a reason.
    pub async fn decide_verification_request(
        &self,
        id: &str,
        approve: bool,
        reason: Option<&str>,
    ) -> Result<Value> {
        let mut tx = self.pool.begin().await?;
        let now = db_now(&mut tx).await?;
        let row = sqlx::query(
            "SELECT account_id FROM verification_requests WHERE id=$1 AND status='pending' FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        let account_id: String = match row {
            Some(row) => row.get("account_id"),
            None => return Err(Error::NotFound),
        };
        if !approve && reason.map(str::trim).unwrap_or("").is_empty() {
            return Err(invalid("A rejection must record a reason"));
        }
        sqlx::query(
            "UPDATE verification_requests SET status=$1,reason=$2,decided_ms=$3 WHERE id=$4",
        )
        .bind(if approve { "approved" } else { "rejected" })
        .bind(reason)
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        if approve {
            let updated = sqlx::query(
                "UPDATE accounts SET role='creator' WHERE id=$1 AND kind='user' AND role='member'",
            )
            .bind(&account_id)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() == 0 {
                return Err(conflict("The requester is no longer an eligible member"));
            }
        }
        audit(
            &mut tx,
            "verification_decision",
            None,
            json!({"request_id": id, "account_id": account_id, "approve": approve, "reason": reason}),
            now,
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"id": id, "account_id": account_id,
            "status": if approve { "approved" } else { "rejected" }}))
    }

    pub async fn instance(&self, id: &str) -> Result<Instance> {
        sqlx::query_as("SELECT * FROM instances WHERE id=$1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Error::NotFound)
    }

    pub async fn instance_detail(&self, id: &str) -> Result<Value> {
        let instance = self.instance(id).await?;
        let mut view = instance_view(&instance, self.now().await?)?;
        if let Some(evidence_id) = &instance.evidence_id {
            let record = sqlx::query(
                "SELECT source_id,event_id,received_ms,payload FROM evidence WHERE id=$1",
            )
            .bind(evidence_id)
            .fetch_one(&self.pool)
            .await?;
            view["evidence"] = json!({"source_id": record.get::<String,_>("source_id"), "event_id": record.get::<String,_>("event_id"),
                "received_ms": record.get::<i64,_>("received_ms"), "payload": record.get::<Json<Value>,_>("payload").0});
        }
        Ok(view)
    }

    /// Time-bucketed price and volume history for one instance (UC-19).
    /// Prices are reconstructed by replaying trades from the opening
    /// inventory, so each point is the price traders saw after the last fill
    /// in that bucket. The first point carries the opening prices; later
    /// points sit at their bucket's end.
    pub async fn instance_history(&self, id: &str, bucket_ms: i64) -> Result<Vec<Value>> {
        let instance = self.instance(id).await?;
        let bucket = bucket_ms.clamp(1_000, 86_400_000);
        let trades = sqlx::query("SELECT outcome_index,side,quantity_millis,amount_micros,created_ms FROM trades WHERE instance_id=$1 ORDER BY created_ms,id LIMIT 5000")
            .bind(id)
            .fetch_all(&self.pool)
            .await?;
        let opening = || -> Result<Value> {
            Ok(
                json!({"start_ms": 0, "prices": amm::prices(&vec![0i64; instance.outcomes.len()], instance.liquidity_units)?, "volume_micros": 0}),
            )
        };
        if trades.is_empty() {
            let mut point = opening()?;
            point["start_ms"] = json!(instance.close_ms - instance.close_ms.rem_euclid(bucket));
            return Ok(vec![point]);
        }
        let mut inventory = vec![0i64; instance.outcomes.len()];
        let mut points = Vec::new();
        let mut volume: i64 = 0;
        let mut current_start: Option<i64> = None;
        for row in &trades {
            let outcome = row.get::<i32, _>("outcome_index") as usize;
            let quantity = row.get::<i64, _>("quantity_millis");
            let amount = row.get::<i64, _>("amount_micros");
            let created = row.get::<i64, _>("created_ms");
            let bucket_start = created - created.rem_euclid(bucket);
            match current_start {
                None => {
                    let mut point = opening()?;
                    point["start_ms"] = json!(bucket_start);
                    points.push(point);
                }
                Some(start) if bucket_start != start => {
                    points.push(json!({"start_ms": start + bucket,
                        "prices": amm::prices(&inventory, instance.liquidity_units)?,
                        "volume_micros": volume}));
                    volume = 0;
                }
                _ => {}
            }
            current_start = Some(bucket_start);
            let direction = if row.get::<String, _>("side") == "buy" {
                1
            } else {
                -1
            };
            inventory[outcome] += direction * quantity;
            volume += amount;
        }
        if let Some(start) = current_start {
            points.push(json!({"start_ms": start + bucket,
                "prices": amm::prices(&inventory, instance.liquidity_units)?,
                "volume_micros": volume}));
        }
        Ok(points)
    }

    pub async fn templates(&self) -> Result<Vec<Value>> {
        let rows =
            sqlx::query("SELECT id,category,title FROM templates ORDER BY category,id LIMIT 100")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.iter().map(|r| json!({"id":r.get::<String,_>("id"),"category":r.get::<String,_>("category"),"title":r.get::<String,_>("title")})).collect())
    }

    pub async fn instances(
        &self,
        template: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>> {
        // Markets that have not closed yet come first, soonest close first
        // (the resolve-soonest order the browse grid wants), followed by
        // closed history, most recently closed first, so settled one-time
        // markets can never bury the live ones on the first page.
        let now = self.now().await?;
        let rows: Vec<Instance> = sqlx::query_as(
            "SELECT * FROM instances WHERE ($1::TEXT IS NULL OR template_id=$1) \
             ORDER BY (close_ms < $4), CASE WHEN close_ms < $4 THEN -close_ms ELSE close_ms END, id \
             LIMIT $2 OFFSET $3",
        )
        .bind(template).bind(limit.clamp(1,100)).bind(offset.clamp(0,100000)).bind(now).fetch_all(&self.pool).await?;
        rows.iter().map(|i| instance_view(i, now)).collect()
    }

    pub async fn create_instance(&self, spec: &NewInstance) -> Result<Instance> {
        let mut tx = self.pool.begin().await?;
        let now = db_now(&mut tx).await?;
        spec.validate(now, self.demo_mode)?;
        let id = Uuid::new_v4().to_string();
        let reserve = format!("reserve:{id}");
        let outcomes = spec.rule.outcomes();
        let funding = amm::funding(spec.liquidity_units, outcomes.len())?;
        // Templates are also the scheduler's uniqueness boundary.
        sqlx::query(
            "INSERT INTO templates(id,category,title) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
        )
        .bind(&spec.template_id)
        .bind(spec.rule.category())
        .bind(&spec.title)
        .execute(&mut *tx)
        .await?;
        let category: String =
            sqlx::query_scalar("SELECT category FROM templates WHERE id=$1 FOR UPDATE")
                .bind(&spec.template_id)
                .fetch_one(&mut *tx)
                .await?;
        if category != spec.rule.category() {
            return Err(invalid("Template category cannot be changed"));
        }
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM instances WHERE template_id=$1 AND close_ms=$2)",
        )
        .bind(&spec.template_id)
        .bind(spec.close_ms)
        .fetch_one(&mut *tx)
        .await?;
        if exists {
            return Err(conflict(
                "An instance already exists for this template and close time",
            ));
        }
        if let Some(creator) = &spec.creator_account_id {
            let kind: Option<String> = sqlx::query_scalar("SELECT kind FROM accounts WHERE id=$1")
                .bind(creator)
                .fetch_optional(&mut *tx)
                .await?;
            if kind.as_deref() != Some("user") {
                return Err(invalid("Creator must be an existing participant account"));
            }
        }
        sqlx::query("INSERT INTO accounts(id,display_name,kind) VALUES($1,$2,'reserve')")
            .bind(&reserve)
            .bind(&spec.title)
            .execute(&mut *tx)
            .await?;
        lock_accounts(&mut tx, &[reserve.clone(), "treasury".into()]).await?;
        if balance(&mut tx, "treasury").await? < funding {
            return Err(conflict("The platform's subsidy budget is exhausted"));
        }
        transfer(
            &mut tx,
            "treasury",
            &reserve,
            funding,
            "subsidy",
            &format!("subsidy:{id}"),
            now,
        )
        .await?;
        sqlx::query("INSERT INTO instances(id,template_id,category,title,resolution_criterion,rule,outcomes,source_id,data_mode,close_ms,observation_start_ms,observation_end_ms,finalize_after_ms,evidence_deadline_ms,liquidity_units,inventory,reserve_account_id,fee_charged,creator_account_id,series_id,bracket_start_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21)")
            .bind(&id).bind(&spec.template_id).bind(spec.rule.category()).bind(&spec.title).bind(&spec.resolution_criterion)
            .bind(Json(&spec.rule)).bind(Json(&outcomes)).bind(&spec.source_id).bind(&spec.data_mode)
            .bind(spec.close_ms).bind(spec.observation_start_ms).bind(spec.observation_end_ms).bind(spec.finalize_after_ms)
            .bind(spec.evidence_deadline_ms).bind(spec.liquidity_units).bind(vec![0i64; outcomes.len()]).bind(&reserve)
            .bind(spec.fee_charged).bind(&spec.creator_account_id).bind(&spec.series_id).bind(spec.bracket_start_ms).execute(&mut *tx).await?;
        event(&mut tx, &id, 0, "opened", now).await?;
        // Scheduler-spawned brackets publish through the outbox; only direct
        // creations are administrator audit records.
        if spec.series_id.is_none() {
            audit(&mut tx, "create_instance", Some(&id), json!(spec), now).await?;
        }
        tx.commit().await?;
        let instance = self.instance(&id).await?;
        self.cache.put_instance(instance.clone());
        Ok(instance)
    }

    /// Publish a market series (ADR 0006). `creator` must hold the creator
    /// role; None seeds a platform-owned series. One-time schedules publish
    /// their single instance immediately.
    pub async fn create_series(
        &self,
        creator: Option<&str>,
        spec: &NewSeries,
        data_mode: &str,
    ) -> Result<Value> {
        if !matches!(data_mode, "manual" | "simulated")
            || (data_mode == "simulated"
                && (!self.demo_mode || spec.source_id != "polyntu-simulator-v1"))
        {
            return Err(invalid(
                "Simulated series require demo mode and source polyntu-simulator-v1; creators publish manual series",
            ));
        }
        let mut tx = self.pool.begin().await?;
        let now = db_now(&mut tx).await?;
        spec.validate(now)?;
        if let Some(crate::market::ResolutionSpec::Creator { .. }) = &spec.resolution {
            // Human authority needs an account to hold the signing key.
            if creator.is_none() {
                return Err(invalid(
                    "Creator resolution authority requires a creator-owned series",
                ));
            }
        }
        if let Some(creator) = creator {
            let record = sqlx::query(
                "SELECT role FROM accounts WHERE id=$1 AND kind='user' FOR NO KEY UPDATE",
            )
            .bind(creator)
            .fetch_optional(&mut *tx)
            .await?;
            if record
                .as_ref()
                .and_then(|row| row.get::<Option<String>, _>("role"))
                .as_deref()
                != Some("creator")
            {
                return Err(Error::Forbidden);
            }
        }
        let id = Uuid::new_v4().to_string();
        // The templates row doubles as the browse grouping for the brackets.
        sqlx::query(
            "INSERT INTO templates(id,category,title) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
        )
        .bind(&id)
        .bind(spec.rule.category())
        .bind(&spec.title)
        .execute(&mut *tx)
        .await?;
        let (recurrence, interval, start_minute, end_minute, days, concurrency, end) =
            match &spec.schedule {
                crate::market::Schedule::Once { .. } => {
                    ("once", None, None, None, vec![1, 2, 3, 4, 5, 6, 7], 1, None)
                }
                crate::market::Schedule::Recurring {
                    interval_ms,
                    active_start_minute,
                    active_end_minute,
                    active_days,
                    max_concurrency,
                    end_ms,
                } => {
                    // Canonical stored form: sorted, deduplicated.
                    let mut days = active_days.clone();
                    days.sort_unstable();
                    days.dedup();
                    (
                        "recurring",
                        Some(*interval_ms),
                        Some(*active_start_minute),
                        Some(*active_end_minute),
                        days,
                        *max_concurrency,
                        *end_ms,
                    )
                }
            };
        let (authority, public_key, endpoint) = match &spec.resolution {
            None => ("admin", None, None),
            Some(crate::market::ResolutionSpec::Creator { public_key }) => {
                ("creator", Some(public_key.trim()), None)
            }
            Some(crate::market::ResolutionSpec::Resolver { endpoint }) => {
                ("resolver", None, Some(endpoint.trim()))
            }
        };
        sqlx::query(
            "INSERT INTO market_series(id,creator_account_id,title,resolution_criterion,rule,source_id,data_mode,liquidity_units,fee_charged,recurrence,interval_ms,active_start_minute,active_end_minute,active_days,max_concurrency,end_ms,anchor_ms,resolution_authority,resolution_public_key,resolver_endpoint,state,created_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,'active',$21)",
        )
        .bind(&id)
        .bind(creator)
        .bind(&spec.title)
        .bind(&spec.resolution_criterion)
        .bind(Json(&spec.rule))
        .bind(&spec.source_id)
        .bind(data_mode)
        .bind(spec.liquidity_units)
        .bind(spec.fee_charged)
        .bind(recurrence)
        .bind(interval)
        .bind(start_minute)
        .bind(end_minute)
        .bind(Json(&days))
        .bind(concurrency)
        .bind(end)
        .bind(now)
        .bind(authority)
        .bind(public_key)
        .bind(endpoint)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        audit(
            &mut tx,
            "create_series",
            None,
            json!({"series_id": id, "creator_account_id": creator, "recurrence": recurrence}),
            now,
        )
        .await?;
        tx.commit().await?;
        if let crate::market::Schedule::Once { .. } = spec.schedule {
            // The single bracket publishes now; if it cannot be funded the
            // series row is removed again so nothing half-published remains.
            let bracket = spec.instance_spec(&id, 0, creator.map(str::to_string), data_mode);
            if let Err(e) = self.create_instance(&bracket).await {
                let _ = sqlx::query("DELETE FROM market_series WHERE id=$1")
                    .bind(&id)
                    .execute(&self.pool)
                    .await;
                return Err(e);
            }
        }
        self.series_view(&id).await
    }

    pub async fn series_view(&self, id: &str) -> Result<Value> {
        let series: Series = sqlx::query_as("SELECT * FROM market_series WHERE id=$1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Error::NotFound)?;
        let rows: Vec<Instance> = sqlx::query_as(
            "SELECT * FROM instances WHERE series_id=$1 ORDER BY close_ms DESC,id LIMIT 100",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        let now = self.now().await?;
        let instances = rows
            .iter()
            .map(|i| instance_view(i, now))
            .collect::<Result<Vec<_>>>()?;
        // Day view (UC-21): per-slot probabilities plus the volume-weighted
        // probability across live brackets, computed on request from the
        // brackets themselves, never stored.
        let volumes: std::collections::HashMap<String, i64> = sqlx::query(
            "SELECT t.instance_id, sum(t.amount_micros)::BIGINT AS volume FROM trades t JOIN instances i ON i.id=t.instance_id WHERE i.series_id=$1 GROUP BY t.instance_id",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|r| (r.get::<String, _>("instance_id"), r.get::<i64, _>("volume")))
        .collect();
        let mut weighted: f64 = 0.0;
        let mut weight_sum: i64 = 0;
        let mut slots = Vec::new();
        for instance in &rows {
            let prices = amm::prices(&instance.inventory, instance.liquidity_units)?;
            let volume = volumes.get(&instance.id).copied().unwrap_or(0);
            if instance.state == "open" && volume > 0 {
                weighted += prices[0] * volume as f64;
                weight_sum += volume;
            }
            slots.push(json!({"bracket_start_ms": instance.bracket_start_ms, "close_ms": instance.close_ms,
                "probability": prices[0], "volume_micros": volume, "state": instance.state,
                "result": instance.result,
                "outcome_id": instance.outcomes[0].id, "outcome_label": instance.outcomes[0].label}));
        }
        let weighted_probability = if weight_sum > 0 {
            json!(weighted / weight_sum as f64)
        } else {
            Value::Null
        };
        Ok(json!({
            "id": series.id, "creator_account_id": series.creator_account_id,
            "title": series.title, "resolution_criterion": series.resolution_criterion,
            "rule": series.rule.0, "source_id": series.source_id, "data_mode": series.data_mode,
            "liquidity_units": series.liquidity_units, "fee_charged": series.fee_charged,
            "state": series.state, "created_ms": series.created_ms,
            "resolution": {"authority": series.resolution_authority,
                "public_key": series.resolution_public_key, "endpoint": series.resolver_endpoint},
            "schedule": {
                "kind": series.recurrence, "interval_ms": series.interval_ms,
                "active_start_minute": series.active_start_minute, "active_end_minute": series.active_end_minute,
                "active_days": series.active_days,
                "max_concurrency": series.max_concurrency, "end_ms": series.end_ms,
            },
            "instances": instances,
            "day": {"weighted_probability": weighted_probability, "slots": slots},
        }))
    }

    pub async fn series_list(&self) -> Result<Vec<Value>> {
        let rows: Vec<Series> =
            sqlx::query_as("SELECT * FROM market_series ORDER BY (state='active') DESC,created_ms DESC,id LIMIT 100")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows
            .iter()
            .map(|s| {
                json!({"id": s.id, "creator_account_id": s.creator_account_id, "title": s.title,
                    "category": s.rule.0.category(), "state": s.state, "recurrence": s.recurrence,
                    "interval_ms": s.interval_ms, "max_concurrency": s.max_concurrency,
                    "fee_charged": s.fee_charged, "end_ms": s.end_ms, "created_ms": s.created_ms})
            })
            .collect())
    }

    /// Rolling spawn (ADR 0006): keep every upcoming grid slot of each active
    /// recurring series published, up to its maximum concurrency, never
    /// outside the active period and never past the series end. Failures are
    /// logged per series so one broken series cannot stall the others.
    pub async fn spawn_due_brackets(&self) -> Result<usize> {
        let now = self.now().await?;
        let series: Vec<Series> = sqlx::query_as(
            "SELECT * FROM market_series WHERE state='active' AND recurrence='recurring'",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut spawned = 0;
        for s in &series {
            match self.spawn_series_brackets(s, now).await {
                Ok(n) => spawned += n,
                Err(e) => {
                    tracing::error!(series_id=%s.id, error=%e, "bracket spawn failed; will retry")
                }
            }
        }
        // A series ends when its last non-terminal bracket settles; a
        // once-series whose single instance is terminal ends too.
        sqlx::query(
            "UPDATE market_series s SET state='ended' WHERE s.state='active' AND (
                (s.recurrence='once' AND EXISTS(SELECT 1 FROM instances i WHERE i.series_id=s.id))
                OR (s.recurrence='recurring' AND s.end_ms IS NOT NULL AND s.end_ms<=$1)
            ) AND NOT EXISTS(SELECT 1 FROM instances i WHERE i.series_id=s.id AND i.state IN ('open','closed','resolving'))",
        )
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(spawned)
    }

    async fn spawn_series_brackets(&self, series: &Series, now: i64) -> Result<usize> {
        let Some(interval) = series.interval_ms else {
            return Ok(0);
        };
        let start_minute = series.active_start_minute.unwrap_or(0);
        let end_minute = series.active_end_minute.unwrap_or(1439);
        // Grid points strictly after now, newest bound by max concurrency.
        let k_min = (now - series.anchor_ms).div_euclid(interval) + 1;
        let mut spawned = 0;
        for k in k_min..k_min + series.max_concurrency {
            let slot = series.anchor_ms + k * interval;
            if series.end_ms.is_some_and(|end| slot >= end) {
                continue;
            }
            if !inside_active_window(slot, start_minute, end_minute) {
                continue;
            }
            // Slots only spawn on the series' operating days (ISO 1 = Monday
            // to 7 = Sunday, Singapore time).
            if !series.active_days.0.contains(&sgt_iso_day(slot)) {
                continue;
            }
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM instances WHERE series_id=$1 AND bracket_start_ms=$2)",
            )
            .bind(&series.id)
            .bind(slot)
            .fetch_one(&self.pool)
            .await?;
            if exists {
                continue;
            }
            match self.create_instance(&series.bracket_spec(slot)).await {
                Ok(_) => spawned += 1,
                Err(Error::Conflict(message))
                    if message == "An instance already exists for this template and close time" => {
                }
                // a competing scheduler won the race
                Err(e) => return Err(e),
            }
        }
        Ok(spawned)
    }

    pub async fn portfolio(&self, account: &str, limit: i64, offset: i64) -> Result<Value> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        let owner: Account = sqlx::query_as(
            "SELECT id,display_name,email,role,balance_micros::TEXT FROM accounts WHERE id=$1",
        )
        .bind(account)
        .fetch_one(&mut *tx)
        .await?;
        let positions = sqlx::query("SELECT p.instance_id,p.outcome_index,p.quantity_millis,i.title,i.state,i.outcomes,i.result,c.credit_micros FROM positions p JOIN instances i ON i.id=p.instance_id LEFT JOIN settlement_claims c ON c.instance_id=p.instance_id AND c.account_id=p.account_id WHERE p.account_id=$1 AND p.quantity_millis>0 ORDER BY i.close_ms DESC,p.instance_id,p.outcome_index LIMIT $2 OFFSET $3")
            .bind(account).bind(limit.clamp(1,100)).bind(offset.clamp(0,100000)).fetch_all(&mut *tx).await?;
        let positions: Vec<Value> = positions.iter().map(|r| {
            let index = r.get::<i32,_>("outcome_index") as usize;
            let outcomes = r.get::<Json<Vec<crate::market::Outcome>>,_>("outcomes");
            json!({"instance_id":r.get::<String,_>("instance_id"),"title":r.get::<String,_>("title"),"state":r.get::<String,_>("state"),
                "outcome_id":outcomes[index].id,"outcome_label":outcomes[index].label,"quantity_millis":r.get::<i64,_>("quantity_millis"),
                "settled":r.get::<Option<i64>,_>("credit_micros").is_some(),"result":r.get::<Option<Json<Value>>,_>("result")})
        }).collect();
        let claims = sqlx::query("SELECT c.instance_id,i.title,c.credit_micros::TEXT,c.created_ms FROM settlement_claims c JOIN instances i ON i.id=c.instance_id WHERE c.account_id=$1 ORDER BY c.created_ms DESC,c.instance_id LIMIT $2 OFFSET $3")
            .bind(account).bind(limit.clamp(1,100)).bind(offset.clamp(0,100000)).fetch_all(&mut *tx).await?;
        let claims: Vec<Value> = claims.iter().map(|r| json!({"instance_id":r.get::<String,_>("instance_id"),"title":r.get::<String,_>("title"),"credit_micros":r.get::<String,_>("credit_micros"),"created_ms":r.get::<i64,_>("created_ms")})).collect();
        tx.commit().await?;
        Ok(json!({"account":owner,"positions":positions,"settlements":claims}))
    }

    pub async fn trades(&self, account: &str, limit: i64, offset: i64) -> Result<Vec<Value>> {
        let rows = sqlx::query("SELECT t.id,t.instance_id,t.outcome_index,t.side,t.quantity_millis,t.amount_micros::TEXT,t.created_ms,i.title,i.outcomes FROM trades t JOIN instances i ON i.id=t.instance_id WHERE t.account_id=$1 ORDER BY t.created_ms DESC,t.id LIMIT $2 OFFSET $3")
            .bind(account).bind(limit.clamp(1,100)).bind(offset.clamp(0,100000)).fetch_all(&self.pool).await?;
        Ok(rows.iter().map(|r| {
            let outcomes = r.get::<Json<Vec<crate::market::Outcome>>,_>("outcomes");
            json!({"id":r.get::<String,_>("id"),"instance_id":r.get::<String,_>("instance_id"),"title":r.get::<String,_>("title"),
                "outcome_label":outcomes[r.get::<i32,_>("outcome_index") as usize].label,"side":r.get::<String,_>("side"),
                "quantity_millis":r.get::<i64,_>("quantity_millis"),"amount_micros":r.get::<String,_>("amount_micros"),"created_ms":r.get::<i64,_>("created_ms")})
        }).collect())
    }

    /// Terminal recurring-series brackets whose evidence deadline passed more
    /// than 24 hours ago are purged with their market rows: outbox entries,
    /// positions, trades, claims, evidence, and the instance. One-time
    /// markets are kept indefinitely. The append-only accounting history
    /// stays untouched: every ledger transfer is shared between two accounts,
    /// so the drained reserve account and its ledger trail remain, as do
    /// admin audit and idempotency rows. A reserve that still holds units
    /// blocks the purge, so a settlement anomaly surfaces instead of
    /// vanishing.
    pub async fn purge_expired_brackets(&self) -> Result<u64> {
        let now = self.now().await?;
        let mut tx = self.pool.begin().await?;
        // The session-local flag opens the guarded delete path of the
        // append-only triggers, and deferring the circular
        // instances/evidence constraint lets both tables go in one
        // transaction. Both revert at commit.
        sqlx::query("SET LOCAL polyntu.purge = 'on'")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SET CONSTRAINTS ALL DEFERRED")
            .execute(&mut *tx)
            .await?;
        let rows = sqlx::query(
            "SELECT i.id, i.reserve_account_id FROM instances i \
             JOIN market_series s ON s.id = i.series_id \
             JOIN accounts r ON r.id = i.reserve_account_id \
             WHERE s.recurrence = 'recurring' AND i.state IN ('resolved','voided') \
             AND i.evidence_deadline_ms <= $1 AND r.balance_micros = 0 LIMIT 20",
        )
        .bind(now - BRACKET_RETENTION_MS)
        .fetch_all(&mut *tx)
        .await?;
        for row in &rows {
            let id: &str = row.get("id");
            sqlx::query("DELETE FROM outbox WHERE instance_id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM positions WHERE instance_id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM trades WHERE instance_id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM settlement_claims WHERE instance_id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM evidence WHERE instance_id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM instances WHERE id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        let purged = rows.len() as u64;
        tx.commit().await?;
        for row in &rows {
            self.cache.invalidate(row.get("id"));
        }
        Ok(purged)
    }

    pub async fn reconcile(&self) -> Result<Value> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        let account_errors: i64 = sqlx::query_scalar("SELECT count(*) FROM accounts a LEFT JOIN (SELECT account_id,sum(delta_micros) AS total FROM ledger_entries GROUP BY account_id) e ON e.account_id=a.id WHERE a.balance_micros <> coalesce(e.total,0)").fetch_one(&mut *tx).await?;
        let inventory_errors: i64 = sqlx::query_scalar("SELECT count(*) FROM instances i CROSS JOIN LATERAL unnest(i.inventory) WITH ORDINALITY q(quantity,idx) WHERE q.quantity <> (SELECT coalesce(sum(p.quantity_millis),0) FROM positions p WHERE p.instance_id=i.id AND p.outcome_index=q.idx-1)").fetch_one(&mut *tx).await?;
        let reserve_errors: i64 =
            sqlx::query_scalar(include_str!("../queries/reconcile_reserves.sql"))
                .fetch_one(&mut *tx)
                .await?;
        let total: String = sqlx::query_scalar("SELECT sum(balance_micros)::TEXT FROM accounts")
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(
            json!({"ok":account_errors==0 && inventory_errors==0 && reserve_errors==0 && total=="0",
            "account_errors":account_errors,"inventory_errors":inventory_errors,"reserve_errors":reserve_errors,"net_units_micros":total}),
        )
    }
}

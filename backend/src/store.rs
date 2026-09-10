use crate::{
    amm, auth,
    error::{Error, Result, conflict, invalid},
    market::{Instance, NewInstance},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, PgConnection, PgPool, Row, postgres::PgPoolOptions, types::Json};
use uuid::Uuid;

#[derive(Clone)]
pub struct Store {
    pub pool: PgPool,
    pub demo_mode: bool,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub display_name: String,
    pub balance_micros: String,
}

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
    sqlx::query("INSERT INTO outbox(instance_id,instance_version,event_type,created_ms) VALUES($1,$2,$3,$4)")
        .bind(instance).bind(version).bind(kind).bind(now).execute(connection).await?;
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
        "liquidity_units": instance.liquidity_units, "result": instance.result,
        "evidence_id": instance.evidence_id, "server_time_ms": now,
        "void_policy": "Each outcome share redeems for 1/n units; aggregate account credits round down to a micro-unit."
    }))
}

impl Store {
    pub async fn connect(url: &str, demo_mode: bool) -> Result<Self> {
        let pool = PgPoolOptions::new().max_connections(24)
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
        let store = Self { pool, demo_mode };
        store.initialize().await?;
        Ok(store)
    }

    async fn initialize(&self) -> Result<()> {
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
            .bind(auth::random_token())
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO accounts(id,display_name,kind) VALUES('issuance','Unit issuance','issuance'),('treasury','Platform subsidy budget','treasury') ON CONFLICT DO NOTHING")
            .execute(&mut *tx).await?;
        let now = db_now(&mut tx).await?;
        sqlx::query("INSERT INTO ledger_transfers(id,from_account,to_account,amount_micros,kind,reference,created_ms) VALUES('initial-issuance','issuance','treasury',1000000000000,'issuance','initial-issuance',$1) ON CONFLICT DO NOTHING")
            .bind(now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn now(&self) -> Result<i64> {
        db_now(&mut *self.pool.acquire().await?).await
    }

    pub async fn account_for_token(&self, token: &str) -> Result<Account> {
        if token.len() > 200 {
            return Err(Error::Unauthorized);
        }
        sqlx::query_as("SELECT id,display_name,balance_micros::TEXT FROM accounts WHERE token_hash=$1 AND kind='user'")
            .bind(auth::hash(token.as_bytes())).fetch_optional(&self.pool).await?.ok_or(Error::Unauthorized)
    }

    pub async fn create_account(&self, display_name: &str) -> Result<Value> {
        let name = display_name.trim();
        if name.len() < 2 || name.len() > 60 || name.chars().any(char::is_control) {
            return Err(invalid(
                "Display name must be 2–60 characters without control characters",
            ));
        }
        let id = Uuid::new_v4().to_string();
        let token = auth::random_token();
        let grant = 1000 * amm::CREDIT_SCALE;
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
            json!({"token": token, "account": {"id": id, "display_name": name, "balance_micros": grant.to_string()}}),
        )
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
        let rows: Vec<Instance> = sqlx::query_as("SELECT * FROM instances WHERE ($1::TEXT IS NULL OR template_id=$1) ORDER BY close_ms DESC,id LIMIT $2 OFFSET $3")
            .bind(template).bind(limit.clamp(1,100)).bind(offset.clamp(0,100000)).fetch_all(&self.pool).await?;
        let now = self.now().await?;
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
        sqlx::query("INSERT INTO instances(id,template_id,category,title,resolution_criterion,rule,outcomes,source_id,data_mode,close_ms,observation_start_ms,observation_end_ms,finalize_after_ms,evidence_deadline_ms,liquidity_units,inventory,reserve_account_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)")
            .bind(&id).bind(&spec.template_id).bind(spec.rule.category()).bind(&spec.title).bind(&spec.resolution_criterion)
            .bind(Json(&spec.rule)).bind(Json(&outcomes)).bind(&spec.source_id).bind(&spec.data_mode)
            .bind(spec.close_ms).bind(spec.observation_start_ms).bind(spec.observation_end_ms).bind(spec.finalize_after_ms)
            .bind(spec.evidence_deadline_ms).bind(spec.liquidity_units).bind(vec![0i64; outcomes.len()]).bind(&reserve).execute(&mut *tx).await?;
        event(&mut tx, &id, 0, "opened", now).await?;
        audit(&mut tx, "create_instance", Some(&id), json!(spec), now).await?;
        tx.commit().await?;
        self.instance(&id).await
    }

    pub async fn portfolio(&self, account: &str, limit: i64, offset: i64) -> Result<Value> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        let owner: Account =
            sqlx::query_as("SELECT id,display_name,balance_micros::TEXT FROM accounts WHERE id=$1")
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

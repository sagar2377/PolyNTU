//! Requires TEST_DATABASE_URL pointing at a PostgreSQL database whose user can CREATE DATABASE.
//! Each test creates and drops its own uniquely named polyntu_test_* database.
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use polyntu::{
    amm::Side,
    api::{AppState, router},
    auth,
    execution::{QuoteRequest, TradeRequest},
    market::{EvidenceInput, Instance, Observation, Resolution, Rule, demo_specs},
    store::{Account, Store, db_now, transfer},
    worker,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const SECRET: &str = "test-quote-secret-012345678901234567890";
const ADMIN: &str = "test-admin-token-012345678901234567890";

struct TestDb {
    store: Store,
    admin: PgPool,
    name: String,
    url: String,
}
impl TestDb {
    async fn new() -> Self {
        let base = std::env::var("TEST_DATABASE_URL")
            .expect("Set TEST_DATABASE_URL for PostgreSQL integration tests");
        let admin = PgPool::connect(&base).await.unwrap();
        let name = format!("polyntu_test_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&admin)
            .await
            .unwrap();
        let (prefix, _) = base
            .rsplit_once('/')
            .expect("Database URL must include a database name");
        let url = format!("{prefix}/{name}");
        let store = Store::connect(&url, true).await.unwrap();
        Self {
            store,
            admin,
            name,
            url,
        }
    }
    fn app(&self) -> Router {
        router(
            AppState::new(self.store.clone(), SECRET.into(), ADMIN.into()).unwrap(),
            vec![],
        )
    }
    async fn account(&self) -> (Account, String) {
        let session = self.store.create_account("Test participant").await.unwrap();
        let token = session["token"].as_str().unwrap().to_owned();
        (self.store.account_for_token(&token).await.unwrap(), token)
    }
    async fn market(&self, rule: Option<Rule>) -> Instance {
        let now = self.store.now().await.unwrap();
        let mut spec = demo_specs(now).remove(1);
        spec.template_id = Uuid::new_v4().to_string();
        spec.data_mode = "manual".into();
        spec.source_id = "test-observer".into();
        spec.close_ms = now + 60000;
        spec.observation_start_ms = now + 60000;
        spec.observation_end_ms = now + 120000;
        spec.finalize_after_ms = now + 240000;
        spec.evidence_deadline_ms = now + 360000;
        if let Some(rule) = rule {
            spec.rule = rule;
        }
        self.store.create_instance(&spec).await.unwrap()
    }
    async fn finish(self) {
        assert_eq!(self.store.reconcile().await.unwrap()["ok"], true);
        self.store.pool.close().await;
        assert!(
            self.name.starts_with("polyntu_test_")
                && self
                    .name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
        );
        sqlx::query(&format!("DROP DATABASE {} WITH (FORCE)", self.name))
            .execute(&self.admin)
            .await
            .unwrap();
        self.admin.close().await;
    }
}

async fn http(
    app: &Router,
    method: &str,
    path: &str,
    body: Value,
    token: Option<&str>,
    admin: bool,
    key: Option<&str>,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    if admin {
        request = request.header("x-admin-token", ADMIN);
    }
    if let Some(key) = key {
        request = request.header("idempotency-key", key);
    }
    let response = app
        .clone()
        .oneshot(
            request
                .body(if body.is_null() {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"text":String::from_utf8_lossy(&bytes)}));
    (status, value)
}

async fn quote(
    db: &TestDb,
    account: &Account,
    instance: &Instance,
    side: Side,
    quantity: i64,
) -> TradeRequest {
    let value = db
        .store
        .quote(
            &account.id,
            &QuoteRequest {
                instance_id: instance.id.clone(),
                outcome_id: instance.outcomes[0].id.clone(),
                side,
                quantity_millis: quantity,
            },
            SECRET,
        )
        .await
        .unwrap();
    TradeRequest {
        quote_token: value["quote_token"].as_str().unwrap().into(),
        limit_micros: value["amount_micros"].as_str().unwrap().into(),
    }
}
async fn buy(db: &TestDb, account: &Account, instance: &Instance, quantity: i64) -> Value {
    let request = quote(db, account, instance, Side::Buy, quantity).await;
    db.store
        .execute(&account.id, &Uuid::new_v4().to_string(), &request, SECRET)
        .await
        .unwrap()
}

#[tokio::test]
async fn foreign_key_read_locks_do_not_block_account_balance_updates() {
    let db = TestDb::new().await;
    let (account, _) = db.account().await;
    let market = db.market(None).await;
    let request = quote(&db, &account, &market, Side::Buy, 1000).await;
    // Reproduce the FK lock held by another in-flight trade's idempotency insert.
    let mut other = db.store.pool.begin().await.unwrap();
    sqlx::query("INSERT INTO idempotency(account_id,key,request_hash) VALUES($1,'pending-other-market','test')")
        .bind(&account.id).execute(&mut *other).await.unwrap();
    let receipt = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        db.store
            .execute(&account.id, "parallel-balance-update", &request, SECRET),
    )
    .await
    .expect("Balance-only locking must remain compatible with FK KEY SHARE")
    .unwrap();
    assert_eq!(receipt["owned_millis"], 1000);
    other.rollback().await.unwrap();
    assert!(Store::connect(&db.url, false).await.is_err());
    db.finish().await;
}
#[tokio::test]
async fn waiting_evidence_cannot_starve_ready_resolution() {
    let db = TestDb::new().await;
    let mut invalid_source = demo_specs(db.store.now().await.unwrap()).remove(0);
    invalid_source.source_id = "unrecognized-simulator".into();
    assert!(db.store.create_instance(&invalid_source).await.is_err());
    let mut waiting = Vec::new();
    for _ in 0..101 {
        waiting.push(db.market(None).await.id);
    }
    let ready = db.market(None).await;
    db.store.advance_demo_clock(5).await.unwrap();
    db.store.close_due().await.unwrap();
    db.store.close_due().await.unwrap();
    db.store
        .ingest_evidence(&ready.id, &rain(&ready, 1, 250))
        .await
        .unwrap();
    worker::tick(&db.store).await.unwrap();
    assert_eq!(
        db.store.instance(&ready.id).await.unwrap().state,
        "resolved"
    );
    assert_eq!(
        db.store.instance(&waiting[0]).await.unwrap().state,
        "closed"
    );
    db.finish().await;
}

fn rain(instance: &Instance, revision: i64, value: i64) -> EvidenceInput {
    EvidenceInput {
        source_id: instance.source_id.clone(),
        event_id: format!("event-{revision}"),
        source_revision: revision,
        window_start_ms: instance.observation_start_ms,
        window_end_ms: instance.observation_end_ms,
        observation: Observation::Weather {
            station_id: "demo-campus".into(),
            total_milli_mm: value,
            complete: true,
        },
        reference: "Recorded test station measurement".into(),
    }
}

#[tokio::test]
async fn http_all_five_categories_buy_sell_resolve_and_authorization() {
    let db = TestDb::new().await;
    worker::seed_demo(&db.store).await.unwrap();
    let (_, token) = db.account().await;
    let app = db.app();
    assert_eq!(
        http(&app, "GET", "/api/v2/me", Value::Null, None, false, None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        http(
            &app,
            "POST",
            "/api/v2/admin/clock/advance",
            json!({"minutes":60}),
            None,
            false,
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        http(&app, "POST", "/contracts", json!({}), None, false, None)
            .await
            .0,
        StatusCode::GONE
    );
    let (status, instances) = http(
        &app,
        "GET",
        "/api/v2/instances",
        Value::Null,
        None,
        false,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(instances.as_array().unwrap().len(), 7);
    for instance in instances.as_array().unwrap() {
        for (side, qty) in [("buy", 10000), ("sell", 5000)] {
            let (status,q)=http(&app,"POST","/api/v2/quotes",json!({"instance_id":instance["id"],"outcome_id":instance["outcomes"][0]["id"],"side":side,"quantity_millis":qty}),Some(&token),false,None).await;
            assert_eq!(status, StatusCode::OK, "{q}");
            let request = json!({"quote_token":q["quote_token"],"limit_micros":q["amount_micros"]});
            let key = Uuid::new_v4().to_string();
            let (status, receipt) = http(
                &app,
                "POST",
                "/api/v2/trades",
                request.clone(),
                Some(&token),
                false,
                Some(&key),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{receipt}");
            assert_eq!(
                http(
                    &app,
                    "POST",
                    "/api/v2/trades",
                    request,
                    Some(&token),
                    false,
                    Some(&key)
                )
                .await
                .1,
                receipt
            );
        }
    }
    let (status, result) = http(
        &app,
        "POST",
        "/api/v2/admin/clock/advance",
        json!({"minutes":2880}),
        None,
        true,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let portfolio = http(
        &app,
        "GET",
        "/api/v2/me/portfolio",
        Value::Null,
        Some(&token),
        false,
        None,
    )
    .await
    .1;
    assert_eq!(portfolio["settlements"].as_array().unwrap().len(), 7);
    assert!(
        portfolio["positions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["settled"] == true)
    );
    worker::tick(&db.store).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM settlement_claims")
        .fetch_one(&db.store.pool)
        .await
        .unwrap();
    assert_eq!(count, 7);
    db.finish().await;
}

#[tokio::test]
async fn concurrent_duplicate_requests_have_one_effect_and_survive_restart() {
    let db = TestDb::new().await;
    let (account, token) = db.account().await;
    let market = db.market(None).await;
    let request = quote(&db, &account, &market, Side::Buy, 10000).await;
    let mut jobs = Vec::new();
    for _ in 0..24 {
        let store = db.store.clone();
        let req = request.clone();
        let id = account.id.clone();
        jobs.push(tokio::spawn(async move {
            store
                .execute(&id, "same-request", &req, SECRET)
                .await
                .unwrap()
        }));
    }
    let first = jobs.remove(0).await.unwrap();
    for job in jobs {
        assert_eq!(job.await.unwrap(), first);
    }
    let reopened = Store::connect(&db.url, true).await.unwrap();
    assert_eq!(
        reopened
            .execute(&account.id, "same-request", &request, SECRET)
            .await
            .unwrap(),
        first
    );
    assert_eq!(
        reopened
            .account_for_token(&token)
            .await
            .unwrap()
            .balance_micros,
        first["balance_micros"]
    );
    let mut different = request.clone();
    different.limit_micros = "0".into();
    assert!(
        reopened
            .execute(&account.id, "same-request", &different, SECRET)
            .await
            .is_err()
    );
    db.store.advance_demo_clock(10).await.unwrap();
    assert_eq!(
        reopened
            .execute(&account.id, "same-request", &request, SECRET)
            .await
            .unwrap(),
        first
    );
    reopened.pool.close().await;
    db.finish().await;
}

#[tokio::test]
async fn competing_quotes_cannot_execute_the_same_inventory_version() {
    let db = TestDb::new().await;
    let (account, _) = db.account().await;
    let market = db.market(None).await;
    let request = quote(&db, &account, &market, Side::Buy, 10000).await;
    let mut jobs = Vec::new();
    for _ in 0..20 {
        let store = db.store.clone();
        let req = request.clone();
        let id = account.id.clone();
        jobs.push(tokio::spawn(async move {
            store
                .execute(&id, &Uuid::new_v4().to_string(), &req, SECRET)
                .await
        }));
    }
    let mut successes = 0;
    for job in jobs {
        if job.await.unwrap().is_ok() {
            successes += 1;
        }
    }
    assert_eq!(successes, 1);
    assert_eq!(
        db.store.instance(&market.id).await.unwrap().inventory[0],
        10000
    );
    db.finish().await;
}

#[tokio::test]
async fn concurrent_cross_market_spending_cannot_overdraw_account() {
    let db = TestDb::new().await;
    let (account, _) = db.account().await;
    let first = db.market(None).await;
    let second = db.market(None).await;
    let mut tx = db.store.pool.begin().await.unwrap();
    let now = db_now(&mut tx).await.unwrap();
    transfer(
        &mut tx,
        &account.id,
        "treasury",
        940_000_000,
        "grant",
        "reduce-test-budget",
        now,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let a = quote(&db, &account, &first, Side::Buy, 80000).await;
    let b = quote(&db, &account, &second, Side::Buy, 80000).await;
    let (a, b) = tokio::join!(
        db.store.execute(&account.id, "a", &a, SECRET),
        db.store.execute(&account.id, "b", &b, SECRET)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    db.finish().await;
}

#[tokio::test]
async fn ownership_expiry_limits_and_pure_quotes_are_enforced() {
    let db = TestDb::new().await;
    let (account, _) = db.account().await;
    let (other, _) = db.account().await;
    let market = db.market(None).await;
    let first = quote(&db, &account, &market, Side::Buy, 10000).await;
    let second = quote(&db, &account, &market, Side::Buy, 10000).await;
    let claims1: polyntu::execution::QuoteClaims =
        auth::verify(&first.quote_token, SECRET.as_bytes()).unwrap();
    let claims2: polyntu::execution::QuoteClaims =
        auth::verify(&second.quote_token, SECRET.as_bytes()).unwrap();
    assert_eq!(claims1.amount_micros, claims2.amount_micros);
    assert_eq!(db.store.instance(&market.id).await.unwrap().version, 0);
    assert!(
        db.store
            .execute(&other.id, "stolen", &first, SECRET)
            .await
            .is_err()
    );
    let mut limited = first.clone();
    limited.limit_micros = "0".into();
    assert!(
        db.store
            .execute(&account.id, "limited", &limited, SECRET)
            .await
            .is_err()
    );
    buy(&db, &account, &market, 10000).await;
    let sell = quote(&db, &other, &market, Side::Sell, 10000).await;
    assert!(
        db.store
            .execute(&other.id, "oversell", &sell, SECRET)
            .await
            .is_err()
    );
    let mut claims: polyntu::execution::QuoteClaims =
        auth::verify(&sell.quote_token, SECRET.as_bytes()).unwrap();
    claims.account_id = account.id.clone();
    claims.expires_ms = db.store.now().await.unwrap() - 1;
    let expired = TradeRequest {
        quote_token: auth::sign(&claims, SECRET.as_bytes()).unwrap(),
        limit_micros: sell.limit_micros,
    };
    assert!(
        db.store
            .execute(&account.id, "expired", &expired, SECRET)
            .await
            .is_err()
    );
    db.finish().await;
}

#[tokio::test]
async fn close_is_checked_after_waiting_for_market_lock() {
    let db = TestDb::new().await;
    let (account, _) = db.account().await;
    let market = db.market(None).await;
    let request = quote(&db, &account, &market, Side::Buy, 10000).await;
    let mut blocker = db.store.pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM instances WHERE id=$1 FOR UPDATE")
        .bind(&market.id)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let store = db.store.clone();
    let id = account.id.clone();
    let job = tokio::spawn(async move { store.execute(&id, "waiting", &request, SECRET).await });
    db.store.advance_demo_clock(2).await.unwrap();
    blocker.commit().await.unwrap();
    assert!(job.await.unwrap().is_err());
    assert_eq!(
        db.store.instance(&market.id).await.unwrap().inventory,
        vec![0, 0]
    );
    db.finish().await;
}

#[tokio::test]
async fn out_of_order_evidence_uses_latest_revision_and_is_idempotent() {
    let db = TestDb::new().await;
    let (account, _) = db.account().await;
    let market = db.market(None).await;
    buy(&db, &account, &market, 10000).await;
    db.store.advance_demo_clock(3).await.unwrap();
    db.store.close_due().await.unwrap();
    let newest = rain(&market, 2, 500);
    let first = db.store.ingest_evidence(&market.id, &newest).await.unwrap();
    assert_eq!(
        db.store.ingest_evidence(&market.id, &newest).await.unwrap()["evidence_id"],
        first["evidence_id"]
    );
    db.store
        .ingest_evidence(&market.id, &rain(&market, 1, 0))
        .await
        .unwrap();
    assert!(
        db.store
            .ingest_evidence(&market.id, &rain(&market, 2, 0))
            .await
            .is_err()
    );
    assert_eq!(db.store.settle_batch(&market.id).await.unwrap(), 0);
    db.store.advance_demo_clock(2).await.unwrap();
    assert_eq!(db.store.settle_batch(&market.id).await.unwrap(), 1);
    assert_eq!(
        db.store
            .instance(&market.id)
            .await
            .unwrap()
            .result
            .unwrap()
            .0,
        Resolution::Winner { outcome: 0 }
    );
    assert!(
        db.store
            .ingest_evidence(&market.id, &rain(&market, 3, 0))
            .await
            .is_err()
    );
    let credit: i64 =
        sqlx::query_scalar("SELECT credit_micros FROM settlement_claims WHERE instance_id=$1")
            .bind(&market.id)
            .fetch_one(&db.store.pool)
            .await
            .unwrap();
    assert_eq!(credit, 10_000_000);
    db.finish().await;
}

#[tokio::test]
async fn missing_evidence_voids_categorical_positions_with_exact_rounding() {
    let db = TestDb::new().await;
    let (account, _) = db.account().await;
    let market = db
        .market(Some(Rule::Election {
            candidates: vec!["A".into(), "B".into(), "C".into()],
            is_fictional: true,
        }))
        .await;
    buy(&db, &account, &market, 1001).await;
    db.store.advance_demo_clock(10).await.unwrap();
    db.store.close_due().await.unwrap();
    db.store.settle_batch(&market.id).await.unwrap();
    assert_eq!(db.store.instance(&market.id).await.unwrap().state, "voided");
    let credit: i64 =
        sqlx::query_scalar("SELECT credit_micros FROM settlement_claims WHERE instance_id=$1")
            .bind(&market.id)
            .fetch_one(&db.store.pool)
            .await
            .unwrap();
    assert_eq!(credit, 333666);
    db.store.settle_batch(&market.id).await.unwrap();
    db.finish().await;
}

#[tokio::test]
async fn failed_trade_rolls_back_ledger_inventory_and_idempotency() {
    let db = TestDb::new().await;
    let (account, token) = db.account().await;
    let market = db.market(None).await;
    let request = quote(&db, &account, &market, Side::Buy, 10000).await;
    sqlx::raw_sql("CREATE FUNCTION fail_trade() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected failure'; END; $$; CREATE TRIGGER fail_trade BEFORE INSERT ON trades FOR EACH ROW EXECUTE FUNCTION fail_trade();").execute(&db.store.pool).await.unwrap();
    assert!(
        db.store
            .execute(&account.id, "rollback", &request, SECRET)
            .await
            .is_err()
    );
    assert_eq!(
        db.store
            .account_for_token(&token)
            .await
            .unwrap()
            .balance_micros,
        account.balance_micros
    );
    assert_eq!(
        db.store.instance(&market.id).await.unwrap().inventory,
        vec![0, 0]
    );
    sqlx::query("DROP TRIGGER fail_trade ON trades")
        .execute(&db.store.pool)
        .await
        .unwrap();
    db.store
        .execute(&account.id, "rollback", &request, SECRET)
        .await
        .unwrap();
    db.finish().await;
}

#[tokio::test]
async fn failed_settlement_batch_can_retry_without_duplicate_credit() {
    let db = TestDb::new().await;
    let (account, token) = db.account().await;
    let market = db.market(None).await;
    buy(&db, &account, &market, 10000).await;
    let before = db
        .store
        .account_for_token(&token)
        .await
        .unwrap()
        .balance_micros;
    db.store.advance_demo_clock(3).await.unwrap();
    db.store.close_due().await.unwrap();
    db.store
        .ingest_evidence(&market.id, &rain(&market, 1, 500))
        .await
        .unwrap();
    db.store.advance_demo_clock(2).await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION fail_claim() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected failure'; END; $$; CREATE TRIGGER fail_claim BEFORE INSERT ON settlement_claims FOR EACH ROW EXECUTE FUNCTION fail_claim();").execute(&db.store.pool).await.unwrap();
    assert!(db.store.settle_batch(&market.id).await.is_err());
    assert_eq!(
        db.store
            .account_for_token(&token)
            .await
            .unwrap()
            .balance_micros,
        before
    );
    sqlx::query("DROP TRIGGER fail_claim ON settlement_claims")
        .execute(&db.store.pool)
        .await
        .unwrap();
    db.store.settle_batch(&market.id).await.unwrap();
    db.store.settle_batch(&market.id).await.unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM settlement_claims WHERE instance_id=$1")
            .bind(&market.id)
            .fetch_one(&db.store.pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    db.finish().await;
}

#[tokio::test]
async fn settlement_resumes_after_a_committed_batch() {
    let db = TestDb::new().await;
    let market = db.market(None).await;
    for _ in 0..101 {
        let (account, _) = db.account().await;
        buy(&db, &account, &market, 1000).await;
    }
    db.store.advance_demo_clock(3).await.unwrap();
    db.store.close_due().await.unwrap();
    db.store
        .ingest_evidence(&market.id, &rain(&market, 1, 500))
        .await
        .unwrap();
    db.store.advance_demo_clock(2).await.unwrap();
    assert_eq!(db.store.settle_batch(&market.id).await.unwrap(), 100);
    assert_eq!(
        db.store.instance(&market.id).await.unwrap().state,
        "resolving"
    );
    assert_eq!(db.store.reconcile().await.unwrap()["ok"], true);
    let reopened = Store::connect(&db.url, true).await.unwrap();
    assert_eq!(reopened.settle_batch(&market.id).await.unwrap(), 1);
    assert_eq!(
        reopened.instance(&market.id).await.unwrap().state,
        "resolved"
    );
    reopened.pool.close().await;
    db.finish().await;
}

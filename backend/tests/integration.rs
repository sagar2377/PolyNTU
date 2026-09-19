//! Requires TEST_DATABASE_URL pointing at a PostgreSQL database whose user can CREATE DATABASE.
//! Each test creates and drops its own uniquely named polyntu_test_* database.
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use polyntu::{
    amm::{self, Side},
    api::{AppState, router},
    auth,
    execution::{QuoteRequest, TradeRequest},
    fee,
    market::{
        EvidenceInput, Instance, NewInstance, NewSeries, Observation, Resolution, ResolutionSpec,
        Rule, Schedule, demo_specs,
    },
    store::{Account, Store, db_now, transfer},
    worker,
};
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgPool};
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
        // The name is polyntu_test_ plus a simple UUID (alphanumeric and
        // underscores only), so interpolating it cannot inject SQL.
        sqlx::query(AssertSqlSafe(format!("CREATE DATABASE {name}")))
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
    async fn creator(&self) -> (Account, String) {
        let session = self
            .store
            .register_account("Creator", "creator@ntu.edu.sg", "correct horse battery")
            .await
            .unwrap();
        let id = session["account"]["id"].as_str().unwrap().to_owned();
        let request = self.store.create_verification_request(&id).await.unwrap();
        self.store
            .decide_verification_request(request["id"].as_str().unwrap(), true, None)
            .await
            .unwrap();
        (
            self.store.account_by_id(&id).await.unwrap(),
            session["token"].as_str().unwrap().to_owned(),
        )
    }
    async fn spec(&self) -> NewInstance {
        let now = self.store.now().await.unwrap();
        let mut spec = demo_specs(now).remove(0);
        spec.template_id = Uuid::new_v4().to_string();
        spec.data_mode = "manual".into();
        spec.source_id = "test-observer".into();
        spec.close_ms = now + 60000;
        spec.observation_start_ms = now + 60000;
        spec.observation_end_ms = now + 120000;
        spec.finalize_after_ms = now + 240000;
        spec.evidence_deadline_ms = now + 360000;
        spec
    }
    async fn market(&self, rule: Option<Rule>) -> Instance {
        let mut spec = self.spec().await;
        if let Some(rule) = rule {
            spec.rule = rule;
        }
        self.store.create_instance(&spec).await.unwrap()
    }
    async fn market_by(&self, creator: &str) -> Instance {
        let mut spec = self.spec().await;
        spec.creator_account_id = Some(creator.into());
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
        sqlx::query(AssertSqlSafe(format!(
            "DROP DATABASE {} WITH (FORCE)",
            self.name
        )))
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

// ADR 0005: NTU email registration, the welcome gift, and the issuance raise.
#[tokio::test]
async fn registration_creates_a_member_account_with_the_welcome_gift() {
    let db = TestDb::new().await;
    let session = db
        .store
        .register_account(
            "Billy Cao",
            "Billy.Cao@SCSE.ntu.edu.sg",
            "correct horse battery",
        )
        .await
        .unwrap();
    assert_eq!(session["account"]["email"], "billy.cao@scse.ntu.edu.sg");
    assert_eq!(session["account"]["role"], "member");
    assert_eq!(session["account"]["balance_micros"], "10000000000");
    let account = db
        .store
        .account_for_token(session["token"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(account.email.as_deref(), Some("billy.cao@scse.ntu.edu.sg"));
    assert_eq!(account.role.as_deref(), Some("member"));
    // Total issuance rose from 1M to 1B units (ADR 0005).
    let issuance: i64 =
        sqlx::query_scalar("SELECT balance_micros FROM accounts WHERE id='issuance'")
            .fetch_one(&db.store.pool)
            .await
            .unwrap();
    assert_eq!(issuance, -(1_000_000_000 * amm::CREDIT_SCALE));
    db.finish().await;
}

#[tokio::test]
async fn registration_rejects_invalid_submissions_and_duplicate_emails() {
    let db = TestDb::new().await;
    for (email, password) in [
        ("billy@gmail.com", "correct horse battery"),
        ("billy@ntu.edu", "correct horse battery"),
        ("@ntu.edu.sg", "correct horse battery"),
        ("billy@ntu.edu.sg", "short"),
    ] {
        assert!(
            db.store
                .register_account("Billy", email, password)
                .await
                .is_err(),
            "should reject {email}"
        );
    }
    db.store
        .register_account("Billy", "billy@ntu.edu.sg", "correct horse battery")
        .await
        .unwrap();
    let duplicate = db
        .store
        .register_account("Billy Two", "BILLY@ntu.edu.sg", "another correct horse")
        .await;
    assert_eq!(
        duplicate.unwrap_err().to_string(),
        "This NTU email is already registered"
    );
    db.finish().await;
}

#[tokio::test]
async fn registration_is_reachable_over_http_and_the_demo_grant_is_unchanged() {
    let db = TestDb::new().await;
    let app = db.app();
    let (status, body) = http(
        &app,
        "POST",
        "/api/v2/auth/register",
        json!({"display_name": "Billy", "email": "billy@ntu.edu.sg", "password": "correct horse battery"}),
        None,
        false,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let token = body["token"].as_str().unwrap().to_owned();
    let (status, me) = http(
        &app,
        "GET",
        "/api/v2/me",
        json!(null),
        Some(&token),
        false,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["email"], "billy@ntu.edu.sg");
    assert_eq!(me["role"], "member");
    assert_eq!(me["balance_micros"], "10000000000");
    let (status, _) = http(
        &app,
        "POST",
        "/api/v2/auth/register",
        json!({"display_name": "Billy", "email": "billy@ntu.edu.sg", "password": "correct horse battery"}),
        None,
        false,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    // The one-click demo account keeps its 1,000-unit development grant.
    let demo = db.store.create_account("Demo user").await.unwrap();
    assert_eq!(demo["account"]["balance_micros"], "1000000000");
    assert!(demo["account"]["email"].is_null());
    db.finish().await;
}

// ADR 0005: password login issues a rotated bearer session token, and an
// account holds at most one live session: every login invalidates all
// previous tokens.
#[tokio::test]
async fn login_rotates_the_session_token() {
    let db = TestDb::new().await;
    let registered = db
        .store
        .register_account("Billy", "billy@ntu.edu.sg", "correct horse battery")
        .await
        .unwrap();
    let registration_token = registered["token"].as_str().unwrap().to_owned();
    // Warm the token cache, then prove login evicts the rotated entry.
    db.store
        .auth_account_for_token(&registration_token)
        .await
        .unwrap();
    let first = db
        .store
        .login("BILLY@ntu.edu.sg", "correct horse battery")
        .await
        .unwrap();
    let first_token = first["token"].as_str().unwrap().to_owned();
    assert_ne!(first_token, registration_token);
    assert_eq!(first["account"]["email"], "billy@ntu.edu.sg");
    assert_eq!(
        first["account"]["balance_micros"],
        registered["account"]["balance_micros"]
    );
    db.store.account_for_token(&first_token).await.unwrap();
    assert!(
        db.store
            .account_for_token(&registration_token)
            .await
            .is_err()
    );
    assert!(
        db.store
            .auth_account_for_token(&registration_token)
            .await
            .is_err()
    );
    // A second login invalidates the first session just the same.
    let second = db
        .store
        .login("billy@ntu.edu.sg", "correct horse battery")
        .await
        .unwrap();
    let second_token = second["token"].as_str().unwrap().to_owned();
    db.store.account_for_token(&second_token).await.unwrap();
    assert!(db.store.account_for_token(&first_token).await.is_err());
    assert!(db.store.auth_account_for_token(&first_token).await.is_err());
    db.finish().await;
}

#[tokio::test]
async fn login_failures_are_indistinguishable_and_demo_accounts_cannot_log_in() {
    let db = TestDb::new().await;
    db.store
        .register_account("Billy", "billy@ntu.edu.sg", "correct horse battery")
        .await
        .unwrap();
    let wrong_password = db
        .store
        .login("billy@ntu.edu.sg", "wrong password")
        .await
        .unwrap_err();
    let unknown_email = db
        .store
        .login("nobody@ntu.edu.sg", "correct horse battery")
        .await
        .unwrap_err();
    assert_eq!(wrong_password.to_string(), "Invalid email or password");
    assert_eq!(unknown_email.to_string(), "Invalid email or password");
    // Demo accounts have no email or password and cannot use the login route.
    db.store.create_account("Demo user").await.unwrap();
    let demo_login = db
        .store
        .login("demo@ntu.edu.sg", "correct horse battery")
        .await;
    assert_eq!(
        demo_login.unwrap_err().to_string(),
        "Invalid email or password"
    );
    db.finish().await;
}

#[tokio::test]
async fn login_is_reachable_over_http() {
    let db = TestDb::new().await;
    let app = db.app();
    http(
        &app,
        "POST",
        "/api/v2/auth/register",
        json!({"display_name": "Billy", "email": "billy@ntu.edu.sg", "password": "correct horse battery"}),
        None,
        false,
        None,
    )
    .await;
    let wrong = http(
        &app,
        "POST",
        "/api/v2/auth/login",
        json!({"email": "billy@ntu.edu.sg", "password": "wrong password"}),
        None,
        false,
        None,
    )
    .await;
    assert_eq!(wrong.0, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.1["error"]["code"], "invalid_credentials");
    let missing = http(
        &app,
        "POST",
        "/api/v2/auth/login",
        json!({"email": "nobody@ntu.edu.sg", "password": "correct horse battery"}),
        None,
        false,
        None,
    )
    .await;
    assert_eq!(missing.0, StatusCode::UNAUTHORIZED);
    assert_eq!(missing.1, wrong.1);
    let (status, session) = http(
        &app,
        "POST",
        "/api/v2/auth/login",
        json!({"email": "BILLY@NTU.EDU.SG", "password": "correct horse battery"}),
        None,
        false,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let token = session["token"].as_str().unwrap();
    let (status, me) = http(
        &app,
        "GET",
        "/api/v2/me",
        json!(null),
        Some(token),
        false,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["email"], "billy@ntu.edu.sg");
    db.finish().await;
}

// ADR 0006: creator-owned market series and rolling spawn.
fn rain_series(schedule: Schedule) -> NewSeries {
    NewSeries {
        title: "Campus rain · afternoon window".into(),
        resolution_criterion: "Yes if total observed rainfall at the campus station reaches 0.2 mm in the published window. Missing samples do not count as zero rainfall.".into(),
        rule: Rule::Weather {
            station_id: "demo-campus".into(),
            threshold_milli_mm: 200,
        },
        source_id: "campus-observer".into(),
        liquidity_units: 100,
        fee_charged: true,
        resolution: None,
        schedule,
    }
}

#[tokio::test]
async fn creators_publish_one_time_series_with_their_single_instance() {
    let db = TestDb::new().await;
    let (creator, token) = db.creator().await;
    let now = db.store.now().await.unwrap();
    let spec = rain_series(Schedule::Once {
        close_ms: now + 60000,
        observation_start_ms: now + 60000,
        observation_end_ms: now + 120000,
        finalize_after_ms: now + 240000,
        evidence_deadline_ms: now + 360000,
    });
    let view = db
        .store
        .create_series(Some(&creator.id), &spec, "manual")
        .await
        .unwrap();
    assert_eq!(view["schedule"]["kind"], "once");
    assert_eq!(view["state"], "active");
    let instances = view["instances"].as_array().unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0]["creator_account_id"], creator.id);
    assert_eq!(instances[0]["series_id"], view["id"]);
    // Members cannot publish series; the route rejects them identically.
    let member = db
        .store
        .register_account("Member", "member@ntu.edu.sg", "correct horse battery")
        .await
        .unwrap();
    let denied = db
        .store
        .create_series(
            Some(member["account"]["id"].as_str().unwrap()),
            &spec,
            "manual",
        )
        .await;
    assert!(matches!(denied, Err(polyntu::error::Error::Forbidden)));
    let app = db.app();
    let body = serde_json::to_value(&spec).unwrap();
    let (status, _) = http(
        &app,
        "POST",
        "/api/v2/series",
        body,
        Some(member["token"].as_str().unwrap()),
        false,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = http(
        &app,
        "POST",
        "/api/v2/series",
        serde_json::to_value(&spec).unwrap(),
        Some(&token),
        false,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    db.finish().await;
}

#[tokio::test]
async fn recurring_series_keep_the_rolling_horizon_filled() {
    let db = TestDb::new().await;
    let (creator, _) = db.creator().await;
    let now = db.store.now().await.unwrap();
    let spec = rain_series(Schedule::Recurring {
        interval_ms: 120000,
        active_start_minute: 0,
        active_end_minute: 1439,
        max_concurrency: 5,
        end_ms: None,
    });
    let view = db
        .store
        .create_series(Some(&creator.id), &spec, "manual")
        .await
        .unwrap();
    let series_id = view["id"].as_str().unwrap().to_owned();
    assert_eq!(view["instances"].as_array().unwrap().len(), 0);
    worker::tick(&db.store).await.unwrap();
    let view = db.store.series_view(&series_id).await.unwrap();
    let instances = view["instances"].as_array().unwrap();
    assert_eq!(
        instances.len(),
        5,
        "the rolling horizon holds max_concurrency brackets"
    );
    let mut closes: Vec<i64> = instances
        .iter()
        .map(|i| i["close_ms"].as_i64().unwrap())
        .collect();
    closes.sort_unstable();
    for pair in closes.windows(2) {
        assert_eq!(
            pair[1] - pair[0],
            120000,
            "brackets sit on the interval grid"
        );
    }
    assert!(closes[0] > now, "brackets close in the future");
    // Two minutes later exactly one new bracket appears on the grid.
    db.store.advance_demo_clock(2).await.unwrap();
    worker::tick(&db.store).await.unwrap();
    let view = db.store.series_view(&series_id).await.unwrap();
    assert_eq!(view["instances"].as_array().unwrap().len(), 6);
    db.finish().await;
}

#[tokio::test]
async fn series_end_stops_spawning_and_settled_brackets_end_the_series() {
    let db = TestDb::new().await;
    let (creator, _) = db.creator().await;
    let now = db.store.now().await.unwrap();
    let spec = rain_series(Schedule::Recurring {
        interval_ms: 60000,
        active_start_minute: 0,
        active_end_minute: 1439,
        max_concurrency: 2,
        end_ms: Some(now + 300000),
    });
    let view = db
        .store
        .create_series(Some(&creator.id), &spec, "manual")
        .await
        .unwrap();
    let series_id = view["id"].as_str().unwrap().to_owned();
    worker::tick(&db.store).await.unwrap();
    let view = db.store.series_view(&series_id).await.unwrap();
    assert_eq!(view["instances"].as_array().unwrap().len(), 2);
    // Past the end and past every deadline: manual brackets without evidence
    // void, and the series ends with them.
    db.store.advance_demo_clock(10).await.unwrap();
    for _ in 0..2 {
        worker::tick(&db.store).await.unwrap();
    }
    let view = db.store.series_view(&series_id).await.unwrap();
    assert_eq!(view["state"], "ended");
    for instance in view["instances"].as_array().unwrap() {
        assert_eq!(instance["state"], "voided");
    }
    // No new brackets appear after the end.
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM instances WHERE series_id=$1")
        .bind(&series_id)
        .fetch_one(&db.store.pool)
        .await
        .unwrap();
    assert_eq!(count, 2);
    db.finish().await;
}

#[tokio::test]
async fn fee_free_markets_charge_no_fee_and_pay_no_creator_share() {
    let db = TestDb::new().await;
    let (creator, _) = db.creator().await;
    let now = db.store.now().await.unwrap();
    let mut spec = rain_series(Schedule::Once {
        close_ms: now + 60000,
        observation_start_ms: now + 60000,
        observation_end_ms: now + 120000,
        finalize_after_ms: now + 240000,
        evidence_deadline_ms: now + 360000,
    });
    // A welfare market: no fee collected, no creator payout.
    spec.fee_charged = false;
    let view = db
        .store
        .create_series(Some(&creator.id), &spec, "manual")
        .await
        .unwrap();
    assert_eq!(view["fee_charged"], false);
    let instance_id = view["instances"][0]["id"].as_str().unwrap().to_owned();
    let (trader, _) = db.account().await;
    let quote = db
        .store
        .quote(
            &trader.id,
            &QuoteRequest {
                instance_id: instance_id.clone(),
                outcome_id: "yes".into(),
                side: Side::Buy,
                quantity_millis: 1000,
            },
            SECRET,
        )
        .await
        .unwrap();
    assert_eq!(quote["fee_micros"], "0");
    let receipt = db
        .store
        .execute(
            &trader.id,
            &Uuid::new_v4().to_string(),
            &TradeRequest {
                quote_token: quote["quote_token"].as_str().unwrap().into(),
                limit_micros: quote["amount_micros"].as_str().unwrap().into(),
            },
            SECRET,
        )
        .await
        .unwrap();
    assert_eq!(receipt["fee_micros"], "0");
    // Settle: the creator's balance is untouched and no fee was ever recorded.
    db.store.advance_demo_clock(7).await.unwrap();
    for _ in 0..2 {
        worker::tick(&db.store).await.unwrap();
    }
    let charged_fees: i64 =
        sqlx::query_scalar("SELECT count(*) FROM trades WHERE instance_id=$1 AND fee_micros>0")
            .bind(&instance_id)
            .fetch_one(&db.store.pool)
            .await
            .unwrap();
    assert_eq!(charged_fees, 0);
    let creator_after = db.store.account_by_id(&creator.id).await.unwrap();
    assert_eq!(creator_after.balance_micros, "10000000000");
    db.finish().await;
}

#[tokio::test]
async fn creators_cannot_trade_in_their_own_markets() {
    let db = TestDb::new().await;
    let (creator, _) = db.creator().await;
    let now = db.store.now().await.unwrap();
    let spec = rain_series(Schedule::Once {
        close_ms: now + 60000,
        observation_start_ms: now + 60000,
        observation_end_ms: now + 120000,
        finalize_after_ms: now + 240000,
        evidence_deadline_ms: now + 360000,
    });
    let view = db
        .store
        .create_series(Some(&creator.id), &spec, "manual")
        .await
        .unwrap();
    let instance_id = view["instances"][0]["id"].as_str().unwrap().to_owned();
    // The creator's own quote is rejected outright.
    let denied = db
        .store
        .quote(
            &creator.id,
            &QuoteRequest {
                instance_id: instance_id.clone(),
                outcome_id: "yes".into(),
                side: Side::Buy,
                quantity_millis: 1000,
            },
            SECRET,
        )
        .await
        .unwrap_err();
    assert_eq!(
        denied.to_string(),
        "Creators cannot trade in their own markets; the fee share is their compensation"
    );
    // Everyone else trades the same market normally.
    let (trader, _) = db.account().await;
    let quote = db
        .store
        .quote(
            &trader.id,
            &QuoteRequest {
                instance_id,
                outcome_id: "yes".into(),
                side: Side::Buy,
                quantity_millis: 1000,
            },
            SECRET,
        )
        .await
        .unwrap();
    let receipt = db
        .store
        .execute(
            &trader.id,
            &Uuid::new_v4().to_string(),
            &TradeRequest {
                quote_token: quote["quote_token"].as_str().unwrap().into(),
                limit_micros: quote["amount_micros"].as_str().unwrap().into(),
            },
            SECRET,
        )
        .await
        .unwrap();
    assert_eq!(receipt["owned_millis"], 1000);
    db.finish().await;
}

#[tokio::test]
async fn the_demo_bus_is_a_rolling_fee_free_series() {
    let db = TestDb::new().await;
    worker::seed_demo(&db.store).await.unwrap();
    // Move the simulated clock inside the 06:00 to 23:59 operating window so
    // the test does not depend on wall-clock time.
    let now = db.store.now().await.unwrap();
    let sgt_minute = (now / 60000 + 480) % 1440;
    if !(360..=1370).contains(&sgt_minute) {
        db.store
            .advance_demo_clock((390 - sgt_minute + 1440) % 1440)
            .await
            .unwrap();
    }
    worker::tick(&db.store).await.unwrap();
    let series_id: String = sqlx::query_scalar(
        "SELECT id FROM market_series WHERE data_mode='simulated' AND rule->>'route_id'='NTU-blue'",
    )
    .fetch_one(&db.store.pool)
    .await
    .unwrap();
    let view = db.store.series_view(&series_id).await.unwrap();
    assert_eq!(view["fee_charged"], false);
    assert_eq!(view["schedule"]["interval_ms"], 120000);
    assert_eq!(view["schedule"]["max_concurrency"], 5);
    assert_eq!(view["schedule"]["active_start_minute"], 360);
    let live: Vec<&Value> = view["instances"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["state"] == "open")
        .collect();
    assert_eq!(
        live.len(),
        5,
        "five live brackets cover a rolling 10 minutes"
    );
    let mut closes: Vec<i64> = live
        .iter()
        .map(|i| i["close_ms"].as_i64().unwrap())
        .collect();
    closes.sort_unstable();
    for pair in closes.windows(2) {
        assert_eq!(pair[1] - pair[0], 120000);
    }
    for bracket in &live {
        assert_eq!(bracket["fee_charged"], false);
        assert_eq!(bracket["category"], "bus");
    }
    // The old one-shot bus template is gone.
    let templates: i64 = sqlx::query_scalar("SELECT count(*) FROM templates WHERE id='bus-blue'")
        .fetch_one(&db.store.pool)
        .await
        .unwrap();
    assert_eq!(templates, 0);
    db.finish().await;
}

// ADR 0007: resolution authority is fixed at creation.
#[tokio::test]
async fn resolution_authority_is_fixed_at_creation_and_blocks_admin_evidence() {
    let db = TestDb::new().await;
    let (creator, _) = db.creator().await;
    let now = db.store.now().await.unwrap();
    // Thirty-two zero bytes: structurally a valid ed25519 public key.
    let public_key = format!("{}=", "A".repeat(43));
    let mut spec = rain_series(Schedule::Once {
        close_ms: now + 60000,
        observation_start_ms: now + 60000,
        observation_end_ms: now + 120000,
        finalize_after_ms: now + 240000,
        evidence_deadline_ms: now + 360000,
    });
    spec.resolution = Some(ResolutionSpec::Creator {
        public_key: public_key.clone(),
    });
    let view = db
        .store
        .create_series(Some(&creator.id), &spec, "manual")
        .await
        .unwrap();
    assert_eq!(view["resolution"]["authority"], "creator");
    assert_eq!(view["resolution"]["public_key"], public_key);
    let instance_id = view["instances"][0]["id"].as_str().unwrap().to_owned();
    let instance = db.store.instance(&instance_id).await.unwrap();
    // The administrator evidence route rejects the creator-owned market.
    let evidence = EvidenceInput {
        source_id: instance.source_id.clone(),
        event_id: "admin-attempt".into(),
        source_revision: 0,
        window_start_ms: instance.observation_start_ms,
        window_end_ms: instance.observation_end_ms,
        observation: Observation::Weather {
            station_id: "demo-campus".into(),
            total_milli_mm: 300,
            complete: true,
        },
        reference: "administrator attempt on a creator-owned market".into(),
    };
    let rejected = db
        .store
        .ingest_evidence(&instance_id, &evidence)
        .await
        .unwrap_err();
    assert_eq!(
        rejected.to_string(),
        "This market's resolution authority is fixed at creation; administrator evidence cannot resolve it"
    );
    // Resolver authority stores the endpoint.
    let mut resolver_spec = rain_series(Schedule::Once {
        close_ms: now + 60000,
        observation_start_ms: now + 60000,
        observation_end_ms: now + 120000,
        finalize_after_ms: now + 240000,
        evidence_deadline_ms: now + 360000,
    });
    resolver_spec.resolution = Some(ResolutionSpec::Resolver {
        endpoint: "https://resolver.example.com/answer".into(),
    });
    let view = db
        .store
        .create_series(Some(&creator.id), &resolver_spec, "manual")
        .await
        .unwrap();
    assert_eq!(view["resolution"]["authority"], "resolver");
    assert_eq!(
        view["resolution"]["endpoint"],
        "https://resolver.example.com/answer"
    );
    // Malformed authority inputs are rejected.
    let mut short_key = resolver_spec.clone();
    short_key.resolution = Some(ResolutionSpec::Creator {
        public_key: "AAAA".into(),
    });
    assert!(
        db.store
            .create_series(Some(&creator.id), &short_key, "manual")
            .await
            .is_err()
    );
    let mut insecure = resolver_spec.clone();
    insecure.resolution = Some(ResolutionSpec::Resolver {
        endpoint: "http://insecure.example.com".into(),
    });
    assert!(
        db.store
            .create_series(Some(&creator.id), &insecure, "manual")
            .await
            .is_err()
    );
    db.finish().await;
}

// The seeded demo administrator reaches admin routes through its session.
#[tokio::test]
async fn demo_databases_seed_an_administrator_account() {
    let db = TestDb::new().await;
    let session = db.store.login("admin@ntu.edu.sg", "admin").await.unwrap();
    assert_eq!(session["account"]["role"], "admin");
    assert_eq!(session["account"]["email"], "admin@ntu.edu.sg");
    let token = session["token"].as_str().unwrap().to_owned();
    let app = db.app();
    let (status, _) = http(
        &app,
        "GET",
        "/api/v2/admin/verification-requests",
        json!(null),
        Some(&token),
        false,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // Member sessions and anonymous requests still cannot.
    let member = db
        .store
        .register_account("Billy", "billy@ntu.edu.sg", "correct horse battery")
        .await
        .unwrap();
    let (status, _) = http(
        &app,
        "GET",
        "/api/v2/admin/verification-requests",
        json!(null),
        Some(member["token"].as_str().unwrap()),
        false,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = http(
        &app,
        "GET",
        "/api/v2/admin/verification-requests",
        json!(null),
        None,
        false,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // The shared admin token keeps working.
    let (status, _) = http(
        &app,
        "GET",
        "/api/v2/admin/verification-requests",
        json!(null),
        None,
        true,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // The administrator does not request creator verification.
    let admin_id = session["account"]["id"].as_str().unwrap().to_owned();
    let request = db
        .store
        .create_verification_request(&admin_id)
        .await
        .unwrap_err();
    assert_eq!(
        request.to_string(),
        "Administrators do not request creator verification"
    );
    db.finish().await;
}

// ADR 0005: the creator verification workflow.
#[tokio::test]
async fn approval_grants_the_creator_role_permanently() {
    let db = TestDb::new().await;
    let session = db
        .store
        .register_account("Billy", "billy@ntu.edu.sg", "correct horse battery")
        .await
        .unwrap();
    let account_id = session["account"]["id"].as_str().unwrap().to_owned();
    let request = db
        .store
        .create_verification_request(&account_id)
        .await
        .unwrap();
    assert_eq!(request["status"], "pending");
    let duplicate = db
        .store
        .create_verification_request(&account_id)
        .await
        .unwrap_err();
    assert_eq!(
        duplicate.to_string(),
        "A pending verification request already exists"
    );
    let pending = db
        .store
        .verification_requests(Some("pending"))
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["email"], "billy@ntu.edu.sg");
    let decision = db
        .store
        .decide_verification_request(request["id"].as_str().unwrap(), true, None)
        .await
        .unwrap();
    assert_eq!(decision["status"], "approved");
    let account = db.store.account_by_id(&account_id).await.unwrap();
    assert_eq!(account.role.as_deref(), Some("creator"));
    // Approval is permanent: re-requesting is rejected, and the decided
    // request cannot be decided again.
    let re_request = db
        .store
        .create_verification_request(&account_id)
        .await
        .unwrap_err();
    assert_eq!(re_request.to_string(), "This account is already a creator");
    let re_decide = db
        .store
        .decide_verification_request(request["id"].as_str().unwrap(), true, None)
        .await
        .unwrap_err();
    assert!(matches!(re_decide, polyntu::error::Error::NotFound));
    // The admin audit recorded the decision.
    let audit_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM admin_audit WHERE action='verification_decision'")
            .fetch_one(&db.store.pool)
            .await
            .unwrap();
    assert_eq!(audit_rows, 1);
    db.finish().await;
}

#[tokio::test]
async fn rejection_records_a_reason_and_allows_reapplication() {
    let db = TestDb::new().await;
    let session = db
        .store
        .register_account("Billy", "billy@ntu.edu.sg", "correct horse battery")
        .await
        .unwrap();
    let account_id = session["account"]["id"].as_str().unwrap().to_owned();
    let first = db
        .store
        .create_verification_request(&account_id)
        .await
        .unwrap();
    let missing_reason = db
        .store
        .decide_verification_request(first["id"].as_str().unwrap(), false, None)
        .await
        .unwrap_err();
    assert_eq!(
        missing_reason.to_string(),
        "A rejection must record a reason"
    );
    db.store
        .decide_verification_request(
            first["id"].as_str().unwrap(),
            false,
            Some("Insufficient campus activity"),
        )
        .await
        .unwrap();
    let account = db.store.account_by_id(&account_id).await.unwrap();
    assert_eq!(account.role.as_deref(), Some("member"));
    let latest = db
        .store
        .verification_request(&account_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(latest["status"], "rejected");
    assert_eq!(latest["reason"], "Insufficient campus activity");
    // A rejected member may apply again, and the second request can succeed.
    let second = db
        .store
        .create_verification_request(&account_id)
        .await
        .unwrap();
    db.store
        .decide_verification_request(second["id"].as_str().unwrap(), true, None)
        .await
        .unwrap();
    let account = db.store.account_by_id(&account_id).await.unwrap();
    assert_eq!(account.role.as_deref(), Some("creator"));
    db.finish().await;
}

#[tokio::test]
async fn verification_endpoints_are_reachable_and_gated_over_http() {
    let db = TestDb::new().await;
    let app = db.app();
    let registered = db
        .store
        .register_account("Billy", "billy@ntu.edu.sg", "correct horse battery")
        .await
        .unwrap();
    let token = registered["token"].as_str().unwrap().to_owned();
    let (status, _) = http(
        &app,
        "POST",
        "/api/v2/verification-requests",
        json!(null),
        None,
        false,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, request) = http(
        &app,
        "POST",
        "/api/v2/verification-requests",
        json!(null),
        Some(&token),
        false,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(request["status"], "pending");
    let (status, _) = http(
        &app,
        "GET",
        "/api/v2/admin/verification-requests",
        json!(null),
        None,
        false,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, list) = http(
        &app,
        "GET",
        "/api/v2/admin/verification-requests?status=pending",
        json!(null),
        None,
        true,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
    let (status, _) = http(
        &app,
        "POST",
        &format!(
            "/api/v2/admin/verification-requests/{}/decision",
            request["id"].as_str().unwrap()
        ),
        json!({"approve": true}),
        None,
        true,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, me) = http(
        &app,
        "GET",
        "/api/v2/me",
        json!(null),
        Some(&token),
        false,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["role"], "creator");
    // Demo accounts must register before they can request verification.
    let demo = db.store.create_account("Demo user").await.unwrap();
    let demo_id = demo["account"]["id"].as_str().unwrap().to_owned();
    let demo_request = db
        .store
        .create_verification_request(&demo_id)
        .await
        .unwrap_err();
    assert_eq!(
        demo_request.to_string(),
        "Register with an NTU email before requesting verification"
    );
    db.finish().await;
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
    assert_eq!(instances.as_array().unwrap().len(), 6);
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
    assert_eq!(portfolio["settlements"].as_array().unwrap().len(), 6);
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
    assert_eq!(count, 6);
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

#[tokio::test]
async fn trading_fees_are_charged_and_split_with_the_creator() {
    let db = TestDb::new().await;
    let (trader, _) = db.account().await;
    let (creator, _) = db.account().await;
    let mut bad = db.spec().await;
    bad.creator_account_id = Some("treasury".into());
    assert!(db.store.create_instance(&bad).await.is_err());
    let market = db.market_by(&creator.id).await;
    let plain = db.market(None).await;
    assert_eq!(
        db.store
            .instance(&market.id)
            .await
            .unwrap()
            .creator_account_id,
        Some(creator.id.clone())
    );

    // The quoted and charged amount is all-in; the fee rides inside it.
    let receipt = buy(&db, &trader, &market, 10000).await;
    assert_eq!(receipt["amount_micros"], "5137761");
    assert_eq!(receipt["fee_micros"], "12813");
    assert_eq!(receipt["balance_micros"], "994862239");
    let engine_sell = amm::calculate(&[10000, 0], 100, 0, Side::Sell, 10000)
        .unwrap()
        .amount_micros;
    let sell_fee = fee::trade_fee(engine_sell);
    let sold = db
        .store
        .execute(
            &trader.id,
            &Uuid::new_v4().to_string(),
            &quote(&db, &trader, &market, Side::Sell, 10000).await,
            SECRET,
        )
        .await
        .unwrap();
    assert_eq!(sold["fee_micros"], sell_fee.to_string());
    assert_eq!(sold["amount_micros"], (engine_sell - sell_fee).to_string());

    // A platform-created market keeps its whole fee pot for the treasury.
    let plain_engine = amm::calculate(&[0, 0], 100, 0, Side::Buy, 5000)
        .unwrap()
        .amount_micros;
    buy(&db, &trader, &plain, 5000).await;

    db.store.advance_demo_clock(3).await.unwrap();
    db.store.close_due().await.unwrap();
    for instance in [&market, &plain] {
        db.store
            .ingest_evidence(&instance.id, &rain(instance, 1, 500))
            .await
            .unwrap();
    }
    db.store.advance_demo_clock(2).await.unwrap();
    for instance in [&market, &plain] {
        db.store.settle_batch(&instance.id).await.unwrap();
        assert_eq!(
            db.store.instance(&instance.id).await.unwrap().state,
            "resolved"
        );
    }
    async fn ledger(pool: &sqlx::PgPool, reference: String) -> i64 {
        sqlx::query_scalar("SELECT amount_micros FROM ledger_transfers WHERE reference=$1")
            .bind(reference)
            .fetch_one(pool)
            .await
            .unwrap()
    }
    let pot = 12813 + sell_fee;
    let creator_cut = fee::creator_share(pot, true);
    assert_eq!(
        ledger(&db.store.pool, format!("fee:{}:creator", market.id)).await,
        creator_cut
    );
    assert_eq!(
        ledger(&db.store.pool, format!("fee:{}:treasury", market.id)).await,
        pot - creator_cut
    );
    assert_eq!(
        ledger(&db.store.pool, format!("fee:{}:treasury", plain.id)).await,
        fee::trade_fee(plain_engine)
    );
    let orphaned: i64 =
        sqlx::query_scalar("SELECT count(*) FROM ledger_transfers WHERE reference=$1")
            .bind(format!("fee:{}:creator", plain.id))
            .fetch_one(&db.store.pool)
            .await
            .unwrap();
    assert_eq!(orphaned, 0);
    // The trader's surviving position settles at one unit per share; the
    // creator account earned only its fee share.
    assert_eq!(
        db.store
            .account_by_id(&trader.id)
            .await
            .unwrap()
            .balance_micros,
        (994_862_239 + engine_sell - sell_fee + 5_000_000
            - fee::charged(plain_engine, true, true).unwrap())
        .to_string()
    );
    assert_eq!(
        db.store
            .account_by_id(&creator.id)
            .await
            .unwrap()
            .balance_micros,
        (1_000_000_000 + creator_cut).to_string()
    );
    for instance in [&market, &plain] {
        let reserve: i64 = sqlx::query_scalar(
            "SELECT a.balance_micros FROM accounts a JOIN instances i ON i.reserve_account_id=a.id WHERE i.id=$1",
        )
        .bind(&instance.id)
        .fetch_one(&db.store.pool)
        .await
        .unwrap();
        assert_eq!(reserve, 0);
    }
    db.finish().await;
}

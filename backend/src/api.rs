use crate::{
    auth,
    error::{Error, Result},
    execution::{QuoteRequest, TradeRequest},
    market::{EvidenceInput, NewInstance},
    store::{Account, Store, instance_view},
    worker,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
};
use futures_core::Stream;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::{convert::Infallible, sync::Arc, time::Duration};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub quote_secret: Arc<str>,
    pub admin_token: Arc<str>,
}

impl AppState {
    pub fn new(store: Store, quote_secret: String, admin_token: String) -> Result<Self> {
        if !auth::valid_secret(&quote_secret)
            || !auth::valid_secret(&admin_token)
            || quote_secret == admin_token
        {
            return Err(crate::error::invalid(
                "Configure distinct quote and admin secrets of at least 32 characters",
            ));
        }
        Ok(Self {
            store,
            quote_secret: quote_secret.into(),
            admin_token: admin_token.into(),
        })
    }
    fn require_admin(&self, headers: &HeaderMap) -> Result<()> {
        let token = headers
            .get("x-admin-token")
            .and_then(|h| h.to_str().ok())
            .ok_or(Error::Forbidden)?;
        if auth::verify_admin(token, &self.admin_token) {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }
    async fn account(&self, headers: &HeaderMap) -> Result<Account> {
        let token = headers
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(Error::Unauthorized)?;
        self.store.account_for_token(token).await
    }
}

pub fn router(state: AppState, origins: Vec<HeaderValue>) -> Router {
    let api = Router::new()
        .route("/config", get(config))
        .route("/auth/demo", post(demo_account))
        .route("/me", get(me))
        .route("/me/portfolio", get(portfolio))
        .route("/me/trades", get(trades))
        .route("/markets", get(markets))
        .route("/markets/{id}/instances", get(template_instances))
        .route("/instances", get(instances))
        .route("/instances/{id}", get(instance))
        .route("/instances/{id}/events", get(events))
        .route("/quotes", post(quote))
        .route("/trades", post(trade))
        .route("/admin/accounts", post(admin_account))
        .route("/admin/instances", post(create_instance))
        .route("/admin/instances/{id}/evidence", post(evidence))
        .route("/admin/instances/{id}/suspension", post(suspend))
        .route("/admin/clock/advance", post(advance))
        .route("/admin/reconcile", get(reconcile))
        .route("/admin/worker/tick", post(tick))
        .fallback(|| async { Error::NotFound });
    Router::new()
        .route("/health", get(health))
        .nest("/api/v2", api)
        .route("/contracts", post(retired).get(retired))
        .route("/markets/{id}/quote", get(retired))
        .route("/markets/{id}/implied-sigma", get(retired))
        .layer(DefaultBodyLimit::max(65536))
        .layer(
            CorsLayer::new()
                .allow_origin(origins)
                .allow_methods([Method::GET, Method::POST])
                .allow_headers([
                    axum::http::header::CONTENT_TYPE,
                    axum::http::header::AUTHORIZATION,
                    axum::http::HeaderName::from_static("idempotency-key"),
                    axum::http::HeaderName::from_static("x-admin-token"),
                ]),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn retired() -> (StatusCode, Json<Value>) {
    (
        StatusCode::GONE,
        Json(
            json!({"error":{"code":"options_retired","message":"Options have been retired. Use /api/v2 outcome markets."}}),
        ),
    )
}
async fn health(State(s): State<AppState>) -> Result<Json<Value>> {
    sqlx::query("SELECT 1").execute(&s.store.pool).await?;
    Ok(Json(
        json!({"status":"ok","backend":"rust","engine":crate::amm::ENGINE_VERSION}),
    ))
}
async fn config(State(s): State<AppState>) -> Result<Json<Value>> {
    Ok(Json(
        json!({"demo_mode":s.store.demo_mode,"server_time_ms":s.store.now().await?,"credit_scale":1000000,"share_scale":1000,"quote_ttl_ms":15000}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountInput {
    display_name: String,
}
async fn demo_account(
    State(s): State<AppState>,
    Json(req): Json<AccountInput>,
) -> Result<Json<Value>> {
    if !s.store.demo_mode {
        return Err(Error::Forbidden);
    }
    Ok(Json(s.store.create_account(&req.display_name).await?))
}
async fn admin_account(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<AccountInput>,
) -> Result<Json<Value>> {
    s.require_admin(&headers)?;
    Ok(Json(s.store.create_account(&req.display_name).await?))
}
async fn me(State(s): State<AppState>, headers: HeaderMap) -> Result<Json<Account>> {
    Ok(Json(s.account(&headers).await?))
}

#[derive(Deserialize, Default)]
struct Page {
    limit: Option<i64>,
    offset: Option<i64>,
}
async fn portfolio(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> Result<Json<Value>> {
    let account = s.account(&headers).await?;
    Ok(Json(
        s.store
            .portfolio(
                &account.id,
                page.limit.unwrap_or(100),
                page.offset.unwrap_or(0),
            )
            .await?,
    ))
}
async fn trades(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> Result<Json<Vec<Value>>> {
    let account = s.account(&headers).await?;
    Ok(Json(
        s.store
            .trades(
                &account.id,
                page.limit.unwrap_or(100),
                page.offset.unwrap_or(0),
            )
            .await?,
    ))
}
async fn markets(State(s): State<AppState>) -> Result<Json<Vec<Value>>> {
    Ok(Json(s.store.templates().await?))
}
async fn template_instances(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(page): Query<Page>,
) -> Result<Json<Vec<Value>>> {
    Ok(Json(
        s.store
            .instances(
                Some(&id),
                page.limit.unwrap_or(100),
                page.offset.unwrap_or(0),
            )
            .await?,
    ))
}
async fn instances(
    State(s): State<AppState>,
    Query(page): Query<Page>,
) -> Result<Json<Vec<Value>>> {
    Ok(Json(
        s.store
            .instances(None, page.limit.unwrap_or(100), page.offset.unwrap_or(0))
            .await?,
    ))
}
async fn instance(State(s): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>> {
    Ok(Json(s.store.instance_detail(&id).await?))
}
async fn quote(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<QuoteRequest>,
) -> Result<Json<Value>> {
    let account = s.account(&headers).await?;
    Ok(Json(s.store.quote(&account, &req, &s.quote_secret).await?))
}
async fn trade(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<TradeRequest>,
) -> Result<Json<Value>> {
    let account = s.account(&headers).await?;
    let key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| crate::error::invalid("Idempotency-Key header required"))?;
    Ok(Json(
        s.store
            .execute(&account.id, key, &req, &s.quote_secret)
            .await?,
    ))
}
async fn create_instance(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<NewInstance>,
) -> Result<Json<Value>> {
    s.require_admin(&headers)?;
    Ok(Json(instance_view(
        &s.store.create_instance(&req).await?,
        s.store.now().await?,
    )?))
}
async fn evidence(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<EvidenceInput>,
) -> Result<Json<Value>> {
    s.require_admin(&headers)?;
    Ok(Json(s.store.ingest_evidence(&id, &req).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SuspensionInput {
    suspended: bool,
    reason: String,
}
async fn suspend(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<SuspensionInput>,
) -> Result<Json<Value>> {
    s.require_admin(&headers)?;
    s.store.suspend(&id, req.suspended, &req.reason).await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdvanceInput {
    minutes: i64,
}
async fn advance(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<AdvanceInput>,
) -> Result<Json<Value>> {
    s.require_admin(&headers)?;
    let now = s.store.advance_demo_clock(req.minutes).await?;
    let settled = worker::tick(&s.store).await?;
    Ok(Json(
        json!({"server_time_ms":now,"settled_accounts":settled}),
    ))
}
async fn reconcile(State(s): State<AppState>, headers: HeaderMap) -> Result<Json<Value>> {
    s.require_admin(&headers)?;
    Ok(Json(s.store.reconcile().await?))
}
async fn tick(State(s): State<AppState>, headers: HeaderMap) -> Result<Json<Value>> {
    s.require_admin(&headers)?;
    Ok(Json(
        json!({"settled_accounts":worker::tick(&s.store).await?}),
    ))
}

#[derive(Deserialize)]
struct EventCursor {
    after: Option<i64>,
}
async fn events(
    State(s): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<EventCursor>,
) -> Result<Sse<impl Stream<Item = std::result::Result<Event, Infallible>>>> {
    s.store.instance(&id).await?;
    let mut cursor = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
        .or(query.after)
        .unwrap_or(0)
        .max(0);
    let stream = async_stream::stream! {
        loop {
            let rows = sqlx::query("SELECT id,instance_version,event_type,created_ms FROM outbox WHERE instance_id=$1 AND id>$2 ORDER BY id LIMIT 100")
                .bind(&id).bind(cursor).fetch_all(&s.store.pool).await;
            match rows {
                Ok(rows) => {
                    for row in rows {
                        cursor = row.get("id");
                        let payload = json!({"instance_id":id,"version":row.get::<i64,_>("instance_version"),"type":row.get::<String,_>("event_type"),"created_ms":row.get::<i64,_>("created_ms")});
                        yield Ok(Event::default().id(cursor.to_string()).event("market").data(payload.to_string()));
                    }
                }
                Err(e) => { tracing::error!(error=%e,"event stream database failure"); break; }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

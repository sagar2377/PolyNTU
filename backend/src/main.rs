use polyntu::{
    api::{AppState, router},
    events,
    store::Store,
    worker,
};
use tower_http::services::ServeDir;

// The multi-thread runtime is the default; the explicit flavour documents
// that request handling, the event listener, and settlement batches run in
// parallel across every available core.
#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "polyntu=info,tower_http=info".into()),
        )
        .init();
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL is required; see docs/development.md")?;
    let quote_secret =
        std::env::var("POLYNTU_QUOTE_SECRET").map_err(|_| "POLYNTU_QUOTE_SECRET is required")?;
    let admin_token =
        std::env::var("POLYNTU_ADMIN_TOKEN").map_err(|_| "POLYNTU_ADMIN_TOKEN is required")?;
    let demo_mode = std::env::var("POLYNTU_DEMO_MODE").is_ok_and(|v| v == "true");
    let address: std::net::SocketAddr = std::env::var("POLYNTU_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8000".into())
        .parse()?;
    if demo_mode && !address.ip().is_loopback() {
        return Err("Demo mode must bind to a loopback address".into());
    }
    let store = Store::connect(&database_url, demo_mode).await?;
    let bus = events::EventBus::default();
    let mut state = AppState::new(store.clone(), quote_secret, admin_token)?;
    state.events = bus.clone();
    if demo_mode {
        worker::seed_demo(&store).await?;
    }
    let origins = std::env::var("POLYNTU_CORS_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:5173,http://127.0.0.1:5173".into())
        .split(',')
        .map(|s| s.trim().parse())
        .collect::<Result<Vec<_>, _>>()?;
    let frontend =
        std::env::var("POLYNTU_FRONTEND_DIR").unwrap_or_else(|_| "../frontend/dist".into());
    let app = router(state, origins).fallback_service(ServeDir::new(frontend));
    let listener = tokio::net::TcpListener::bind(address).await?;
    let worker = tokio::spawn(worker::run(store.clone()));
    let events_task = tokio::spawn(events::run_listener(store.clone(), bus));
    let clock_task = tokio::spawn(events::run_clock_refresher(store));
    tracing::info!(%address,demo_mode,"PolyNTU Rust backend listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    worker.abort();
    events_task.abort();
    clock_task.abort();
    Ok(())
}

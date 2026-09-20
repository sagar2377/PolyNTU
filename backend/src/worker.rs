use crate::{
    error::{Error, Result},
    market::{EvidenceInput, Instance, NewSeries, Rule, Series, demo_series_specs, demo_specs},
    resolver,
    store::Store,
};
use serde_json::{Value, json};

/// The (route, stop) pair a bus series spec covers, for seed matching.
fn bus_route_stop(spec: &NewSeries) -> (&str, &str) {
    match &spec.rule {
        Rule::Bus {
            route_id, stop_id, ..
        } => (route_id, stop_id),
        _ => unreachable!("bus specs carry bus rules"),
    }
}

pub async fn seed_demo(store: &Store) -> Result<()> {
    if !store.demo_mode {
        return Ok(());
    }
    let now = store.now().await?;
    // The bus demos are rolling, fee-free welfare series (ADR 0006) covering
    // the campus shuttle lines, each on its published operating days and
    // hours; brackets resolve through the external-resolver contract (ADR
    // 0007): the settlement worker asks the platform's own NTU Bus API
    // adapter, served by this process, over HTTP like any other resolver.
    // The URL points back at the loopback address demo mode binds. The series
    // are owned by the seeded bus market creator, bus@ntu.edu.sg, so the
    // creator experience can be demonstrated.
    const DEMO_BUS_CREATOR: &str = "demo-bus";
    let bind = std::env::var("POLYNTU_BIND").unwrap_or_else(|_| "127.0.0.1:8000".into());
    let endpoint = format!("http://{bind}/api/v2/resolvers/ntu-bus");
    let specs = demo_series_specs(&endpoint);
    let active: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT rule->>'route_id', rule->>'stop_id', creator_account_id FROM market_series WHERE data_mode='simulated' AND rule->>'kind'='bus' AND state='active'",
    )
    .fetch_all(&store.pool)
    .await?;
    let matches_spec = |route: &str, stop: &str, creator: Option<&str>| {
        specs
            .iter()
            .any(|spec| bus_route_stop(spec) == (route, stop) && creator == Some(DEMO_BUS_CREATOR))
    };
    // Definitions are immutable, so a database seeded with different stops,
    // routes, or ownership keeps its old bus series but ended; the current
    // specs take over the same lines.
    for (route, stop, creator) in &active {
        if !matches_spec(route, stop, creator.as_deref()) {
            sqlx::query(
                "UPDATE market_series SET state='ended' WHERE data_mode='simulated' AND rule->>'kind'='bus' AND state='active' AND rule->>'route_id'=$1 AND rule->>'stop_id'=$2",
            )
            .bind(route)
            .bind(stop)
            .execute(&store.pool)
            .await?;
        }
    }
    for spec in &specs {
        if !active.iter().any(|(route, stop, creator)| {
            bus_route_stop(spec) == (route.as_str(), stop.as_str())
                && creator.as_deref() == Some(DEMO_BUS_CREATOR)
        }) {
            store
                .create_series(Some(DEMO_BUS_CREATOR), spec, "simulated")
                .await?;
        }
    }
    for spec in demo_specs(now) {
        // One upcoming occurrence per template; elections are a single demo event.
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM instances WHERE template_id=$1 AND (close_ms>$2 OR category='elections'))")
            .bind(&spec.template_id).bind(now).fetch_one(&store.pool).await?;
        if !exists {
            match store.create_instance(&spec).await {
                Ok(_) => {}
                Err(Error::Conflict(message))
                    if message == "An instance already exists for this template and close time" => {
                } // competing scheduler
                Err(e) => return Err(e),
            }
        }
    }
    Ok(())
}

pub async fn tick(store: &Store) -> Result<usize> {
    // One settlement pass at a time. The admin clock-advance and worker-tick
    // endpoints run this pass on demand; a concurrent pass would select the
    // same due instances, so each resolver answer would be recorded twice and
    // the losing record rejected as already final.
    let _pass = store.tick_lock.lock().await;
    store.close_due().await?;
    store.spawn_due_brackets().await?;
    let now = store.now().await?;
    // Select actionable rows so markets awaiting evidence cannot fill the batch
    // indefinitely and starve later instances whose results are ready.
    // Resolver-authority instances become due at their finalize window so the
    // worker can ask their external source (ADR 0007).
    let due: Vec<Instance> = sqlx::query_as("SELECT i.* FROM instances i LEFT JOIN evidence e ON e.id=i.evidence_id WHERE i.state='resolving' OR (i.state='closed' AND ((i.data_mode='simulated' AND $2 AND i.evidence_id IS NULL AND i.observation_end_ms<=$1) OR (i.finalize_after_ms<=$1 AND (i.evidence_deadline_ms<=$1 OR e.evaluated_result IS NOT NULL)) OR (i.finalize_after_ms<=$1 AND i.evidence_id IS NULL AND EXISTS(SELECT 1 FROM market_series s WHERE s.id=i.series_id AND s.resolution_authority='resolver')))) ORDER BY i.finalize_after_ms,i.id LIMIT 100")
        .bind(now).bind(store.demo_mode).fetch_all(&store.pool).await?;
    let mut count = 0;
    // Independent instances settle concurrently in bounded chunks; the
    // instance row lock keeps each instance internally serial.
    for chunk in due.chunks(SETTLE_CONCURRENCY) {
        let mut tasks = tokio::task::JoinSet::new();
        for instance in chunk {
            let store = store.clone();
            let instance = instance.clone();
            tasks.spawn(async move { process_instance(&store, &instance).await });
        }
        while let Some(joined) = tasks.join_next().await {
            match joined {
                Ok(Ok(n)) => count += n,
                Ok(Err(e)) => tracing::error!(error = %e, "settlement task failed"),
                Err(e) => tracing::error!(error = %e, "settlement task panicked"),
            }
        }
    }
    // Terminal recurring brackets past the retention window are purged in
    // small batches; one-time markets stay forever. A purge failure must not
    // stall settlement, so it is logged and retried on the next tick.
    if let Err(e) = store.purge_expired_brackets().await {
        tracing::error!(error = %e, "bracket purge failed");
    }
    seed_demo(store).await?;
    Ok(count)
}

const SETTLE_CONCURRENCY: usize = 8;

/// Simulated evidence for one closed demo instance, then one settlement batch.
/// Logs and continues on failure; the next tick retries.
async fn process_instance(store: &Store, instance: &Instance) -> Result<usize> {
    // ADR 0007: resolver-authority instances ask their external source at the
    // finalize window. A valid answer settles; pending, malformed, or
    // unreachable sources retry on later ticks until the published deadline
    // voids the instance. The simulated-evidence fallback never applies to
    // them, so a broken resolver is visible instead of masked.
    let series: Option<Series> = match &instance.series_id {
        Some(series_id) => {
            sqlx::query_as("SELECT * FROM market_series WHERE id=$1")
                .bind(series_id)
                .fetch_optional(&store.pool)
                .await?
        }
        None => None,
    };
    if store.demo_mode
        && instance.data_mode == "simulated"
        && instance.state == "closed"
        && instance.evidence_id.is_none()
        && series
            .as_ref()
            .is_none_or(|s| s.resolution_authority != "resolver")
    {
        let secret: String =
            sqlx::query_scalar("SELECT simulation_secret FROM settings WHERE singleton")
                .fetch_one(&store.pool)
                .await?;
        let private_seed = crate::auth::hash(format!("{secret}:{}", instance.id).as_bytes());
        let input = EvidenceInput { source_id: instance.source_id.clone(), event_id: format!("simulated:{}", instance.id), source_revision: 0,
            window_start_ms: instance.observation_start_ms, window_end_ms: instance.observation_end_ms,
            observation: instance.rule.simulated(&private_seed, instance.observation_start_ms, instance.observation_end_ms),
            reference: "Deterministic demo observation, generated after the observation window; no live source".into() };
        match store.record_evidence(&instance.id, &input, true).await {
            Ok(_) | Err(Error::Conflict(_)) => {}
            Err(e) => {
                tracing::error!(instance_id=%instance.id, error=%e, "simulated observation failed");
            }
        }
    }
    if instance.state == "closed"
        && instance.evidence_id.is_none()
        && let Some(series) = series.filter(|s| s.resolution_authority == "resolver")
        && let Some(endpoint) = series.resolver_endpoint.clone()
    {
        let request = resolver::ResolverRequest::of(instance);
        match resolver::call(&endpoint, &request).await {
            Ok((body, request_value)) => {
                let response = serde_json::from_slice::<Value>(&body)
                    .unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&body).to_string()}));
                match resolver::parse_response(&body, instance) {
                    resolver::ResolverAnswer::Outcome(outcome_id) => {
                        // A deterministic simulated answer recorded after a
                        // demo clock jump is a replay, exactly like the
                        // simulated-evidence branch; real resolvers keep the
                        // hard published deadline.
                        if let Err(e) = store
                            .record_resolver_evidence(
                                &instance.id,
                                &outcome_id,
                                &request_value,
                                &response,
                                store.demo_mode && instance.data_mode == "simulated",
                            )
                            .await
                        {
                            tracing::error!(instance_id=%instance.id, error=%e, "resolver evidence failed");
                        }
                    }
                    resolver::ResolverAnswer::Pending => {}
                    resolver::ResolverAnswer::Invalid => {
                        tracing::warn!(instance_id=%instance.id, "resolver answer is invalid; retrying until the deadline");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(instance_id=%instance.id, error=%e, "resolver unreachable; retrying until the deadline");
            }
        }
    }
    match store.settle_batch(&instance.id).await {
        Ok(n) => Ok(n),
        Err(e) => {
            tracing::error!(instance_id=%instance.id, error=%e, "settlement batch failed; will retry");
            Ok(0)
        }
    }
}

pub async fn run(store: Store) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        if let Err(e) = tick(&store).await {
            tracing::error!(error=%e, "worker cycle failed; will retry");
        }
    }
}

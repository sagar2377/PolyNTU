use crate::{
    error::{Error, Result},
    market::{EvidenceInput, Instance, demo_specs},
    store::Store,
};

pub async fn seed_demo(store: &Store) -> Result<()> {
    if !store.demo_mode {
        return Ok(());
    }
    let now = store.now().await?;
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
    store.close_due().await?;
    let now = store.now().await?;
    // Select actionable rows so markets awaiting evidence cannot fill the batch
    // indefinitely and starve later instances whose results are ready.
    let due: Vec<Instance> = sqlx::query_as("SELECT i.* FROM instances i LEFT JOIN evidence e ON e.id=i.evidence_id WHERE i.state='resolving' OR (i.state='closed' AND ((i.data_mode='simulated' AND $2 AND i.evidence_id IS NULL AND i.observation_end_ms<=$1) OR (i.finalize_after_ms<=$1 AND (i.evidence_deadline_ms<=$1 OR e.evaluated_result IS NOT NULL)))) ORDER BY i.finalize_after_ms,i.id LIMIT 100")
        .bind(now).bind(store.demo_mode).fetch_all(&store.pool).await?;
    let mut count = 0;
    for instance in due {
        if store.demo_mode
            && instance.data_mode == "simulated"
            && instance.state == "closed"
            && instance.evidence_id.is_none()
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
        match store.settle_batch(&instance.id).await {
            Ok(n) => count += n,
            Err(e) => {
                tracing::error!(instance_id=%instance.id, error=%e, "settlement batch failed; will retry")
            }
        }
    }
    seed_demo(store).await?;
    Ok(count)
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

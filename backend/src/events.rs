//! Push delivery for market events.
//!
//! Committed outbox rows are broadcast through PostgreSQL `LISTEN/NOTIFY` to
//! every connected process, which fans them out to its SSE subscribers. The
//! durable outbox stays authoritative: each stream still replays from it on
//! connect and re-checks it on a slow interval, so a lost or lagged
//! notification delays an event, it never drops it.
use crate::{cache::Cache, store::Store};
use serde::Deserialize;
use serde_json::json;
use sqlx::postgres::PgListener;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::sync::broadcast;

pub const CHANNEL: &str = "polyntu_events";

/// One committed outbox row as published by the notification payload.
#[derive(Debug, Clone, Deserialize)]
pub struct OutboxEvent {
    pub id: i64,
    pub instance_id: String,
    pub version: i64,
    #[serde(rename = "type")]
    pub event_type: String,
    pub created_ms: i64,
    /// Present on trade events so caches can update without a database read.
    pub inventory: Option<Vec<i64>>,
}

impl OutboxEvent {
    /// Matches the payload shape the SSE handler has always emitted.
    pub fn sse_data(&self) -> String {
        json!({
            "instance_id": self.instance_id,
            "version": self.version,
            "type": self.event_type,
            "created_ms": self.created_ms,
        })
        .to_string()
    }
}

/// Per-instance broadcast channels shared by all SSE subscribers.
#[derive(Debug, Clone, Default)]
pub struct EventBus {
    channels: Arc<RwLock<HashMap<String, broadcast::Sender<Arc<OutboxEvent>>>>>,
}

impl EventBus {
    pub fn subscribe(&self, instance: &str) -> broadcast::Receiver<Arc<OutboxEvent>> {
        let mut map = self.channels.write().unwrap();
        let sender = map
            .entry(instance.to_string())
            .or_insert_with(|| broadcast::channel(1024).0);
        sender.subscribe()
    }

    fn publish(&self, event: Arc<OutboxEvent>) {
        let sender = {
            let map = self.channels.read().unwrap();
            map.get(&event.instance_id).cloned()
        };
        if let Some(sender) = sender
            && sender.send(event.clone()).is_err()
        {
            self.channels.write().unwrap().remove(&event.instance_id);
        }
    }
}

fn handle(cache: &Cache, bus: &EventBus, payload: &str) {
    match serde_json::from_str::<OutboxEvent>(payload) {
        Ok(event) => {
            let event = Arc::new(event);
            if let Some(inventory) = &event.inventory {
                cache.apply_trade(&event.instance_id, event.version, inventory);
            } else {
                cache.invalidate(&event.instance_id);
            }
            bus.publish(event);
        }
        Err(e) => tracing::warn!(error = %e, "unreadable event notification payload"),
    }
}

/// Dedicated listener connection that fans committed events out and keeps the
/// instance cache converged. Reconnects with a delay on connection loss.
pub async fn run_listener(store: Store, bus: EventBus) {
    loop {
        match PgListener::connect_with(&store.pool).await {
            Ok(mut listener) => {
                if let Err(e) = listener.listen(CHANNEL).await {
                    tracing::error!(channel = CHANNEL, error = %e, "event listener LISTEN failed");
                } else {
                    tracing::info!(channel = CHANNEL, "event listener connected");
                    loop {
                        match listener.recv().await {
                            Ok(notification) => {
                                handle(&store.cache, &bus, notification.payload());
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "event listener disconnected");
                                break;
                            }
                        }
                    }
                }
            }
            Err(e) => tracing::error!(error = %e, "event listener connection failed"),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Demo mode only: keep the cached clock offset converged even if another
/// process advanced the shared clock.
pub async fn run_clock_refresher(store: Store) {
    if !store.demo_mode {
        return;
    }
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match sqlx::query_scalar::<_, i64>("SELECT clock_offset_ms FROM settings WHERE singleton")
            .fetch_one(&store.pool)
            .await
        {
            Ok(offset) => store.cache.set_clock_offset(offset),
            Err(e) => tracing::warn!(error = %e, "clock offset refresh failed"),
        }
    }
}

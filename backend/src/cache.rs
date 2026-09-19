//! Read-through caches for the hot request paths.
//!
//! Quotes are previews by design: execution re-validates every claim inside the
//! database transaction, so a briefly stale entry can only produce a quote that
//! execution rejects, never a wrong trade. All mutating paths invalidate or
//! write through this cache before returning, and the PostgreSQL notification
//! listener applies the same updates for writes made by other processes.
use crate::market::Instance;
use std::{
    collections::HashMap,
    sync::{
        Arc, RwLock,
        atomic::{AtomicI64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const INSTANCE_TTL: Duration = Duration::from_secs(2);
const INSTANCE_CAPACITY: usize = 10_000;
const TOKEN_CAPACITY: usize = 50_000;

/// The immutable identity an authentication token resolves to. Balances are
/// intentionally excluded: they change on every trade and are read fresh.
#[derive(Debug, Clone)]
pub struct AuthAccount {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Default)]
struct Inner {
    instances: RwLock<HashMap<String, (Arc<Instance>, Instant)>>,
    tokens: RwLock<HashMap<String, Arc<AuthAccount>>>,
    clock_offset_ms: AtomicI64,
}

#[derive(Debug, Clone, Default)]
pub struct Cache {
    inner: Arc<Inner>,
}

impl Cache {
    pub fn new(clock_offset_ms: i64) -> Self {
        let cache = Self::default();
        cache
            .inner
            .clock_offset_ms
            .store(clock_offset_ms, Ordering::Relaxed);
        cache
    }

    pub fn instance(&self, id: &str) -> Option<Arc<Instance>> {
        let map = self.inner.instances.read().unwrap();
        map.get(id)
            .filter(|(_, stored)| stored.elapsed() < INSTANCE_TTL)
            .map(|(instance, _)| instance.clone())
    }

    pub fn put_instance(&self, instance: Instance) -> Arc<Instance> {
        let shared = Arc::new(instance);
        let mut map = self.inner.instances.write().unwrap();
        if map.len() >= INSTANCE_CAPACITY {
            map.clear();
        }
        map.insert(shared.id.clone(), (shared.clone(), Instant::now()));
        shared
    }

    /// Apply a committed trade to a cached instance. The version guard makes
    /// this idempotent when the in-process write-through and the notification
    /// listener deliver the same update twice.
    pub fn apply_trade(&self, id: &str, version: i64, inventory: &[i64]) {
        let mut map = self.inner.instances.write().unwrap();
        if let Some((entry, stored)) = map.get_mut(id) {
            if entry.version == version - 1 {
                let instance = Arc::make_mut(entry);
                instance.inventory = inventory.to_vec();
                instance.version = version;
                *stored = Instant::now();
            } else if entry.version < version {
                // An intermediate update was missed; refetch instead of guessing.
                map.remove(id);
            }
        }
    }

    pub fn invalidate(&self, id: &str) {
        self.inner.instances.write().unwrap().remove(id);
    }

    pub fn account(&self, token_hash: &str) -> Option<Arc<AuthAccount>> {
        self.inner.tokens.read().unwrap().get(token_hash).cloned()
    }

    pub fn put_account(&self, token_hash: String, account: AuthAccount) -> Arc<AuthAccount> {
        let shared = Arc::new(account);
        let mut map = self.inner.tokens.write().unwrap();
        if map.len() >= TOKEN_CAPACITY {
            map.clear();
        }
        map.insert(token_hash, shared.clone());
        shared
    }

    /// Drop a rotated token so it stops resolving immediately in this
    /// process; the database row already rejects it everywhere else.
    pub fn evict_account(&self, token_hash: &str) {
        self.inner.tokens.write().unwrap().remove(token_hash);
    }

    pub fn set_clock_offset(&self, offset_ms: i64) {
        self.inner
            .clock_offset_ms
            .store(offset_ms, Ordering::Relaxed);
    }

    /// Local estimate of the database clock. The offset is refreshed on
    /// startup, on every demo-clock advance, and periodically in demo mode.
    pub fn now_ms(&self) -> i64 {
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or_default();
        epoch + self.inner.clock_offset_ms.load(Ordering::Relaxed)
    }
}

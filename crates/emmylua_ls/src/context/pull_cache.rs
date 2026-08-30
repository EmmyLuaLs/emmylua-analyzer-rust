//! # Pull request coalescer
//!
//! Only responsible for serial coalescing of same-key requests and latest-wins cancellation.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Default)]
pub struct RequestManager {
    inflight: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    latest: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl RequestManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Acquire the per-key serial lock used to coalesce recent duplicate pull requests.
    pub async fn lock_for(&self, key: &str) -> Arc<Mutex<()>> {
        let mut map = self.inflight.lock().await;
        map.entry(key.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Register a new latest request: cancel the old same-key token so it backs off quickly.
    pub async fn begin(&self, key: &str, token: CancellationToken) {
        let mut map = self.latest.lock().await;
        if let Some(old) = map.insert(key.to_string(), token) {
            old.cancel();
        }
    }
}

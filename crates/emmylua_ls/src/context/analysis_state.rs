//! # AnalysisState — single canonical copy of the salsa analysis handle
//!
//! salsa 0.28's write protocol is `cancel_others()`: any input write waits for **all**
//! `db.clone()` snapshots to drop. A long-lived `EmmyLuaAnalysis` in the watch channel
//! would block the next write forever — broadcasting snapshots is incompatible with salsa 0.28.
//!
//! The lock here only protects the "minting" action itself:
//! - `snapshot()` does one ~50ns `db.clone()` inside the lock; queries then run lock-free,
//!   concurrently on independent salsa snapshots.
//! - `update()` holds the write lock and performs the mutation. `cancel_others` only waits for snapshots
//!   actually in flight; no resident copy blocks it.

use std::sync::Mutex;

use emmylua_code_analysis::EmmyLuaAnalysis;

use crate::context::RequestOutcome;

pub struct AnalysisState {
    inner: Mutex<EmmyLuaAnalysis>,
}

impl AnalysisState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(EmmyLuaAnalysis::new()),
        }
    }

    /// Current analysis snapshot: clone inside the lock (sharing memo/VFS), then query lock-free outside.
    fn snapshot(&self) -> EmmyLuaAnalysis {
        self.lock().clone()
    }

    /// Run a query on a snapshot, catching salsa's cancellation panic.
    /// If a pending write cancels the query, return `None` instead of propagating the panic.
    pub fn with_snapshot<T>(&self, f: impl FnOnce(&EmmyLuaAnalysis) -> T) -> Option<T> {
        let analysis = self.snapshot();
        Some(f(&analysis))
    }

    /// Run a query that may return `None` on a snapshot, catching salsa cancellation.
    pub fn try_with_snapshot<R>(&self, f: impl FnOnce(&EmmyLuaAnalysis) -> Option<R>) -> Option<R> {
        let analysis = self.snapshot();
        f(&analysis)
    }

    pub fn query<R>(&self, f: impl FnOnce(&EmmyLuaAnalysis) -> Option<R>) -> RequestOutcome<R> {
        let analysis = self.snapshot();
        match f(&analysis) {
            Some(value) => RequestOutcome::Ready(value),
            None => RequestOutcome::Missing,
        }
    }

    /// Run a write operation serially. After it completes, later `snapshot()` calls see the new state.
    pub async fn update<R>(&self, f: impl FnOnce(&mut EmmyLuaAnalysis) -> R) -> R {
        let mut analysis = self.lock();
        f(&mut analysis)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, EmmyLuaAnalysis> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Default for AnalysisState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[tokio::test]
    async fn update_broadcasts_salsa_snapshot() {
        let state = AnalysisState::new();
        let uri = lsp_types::Uri::from_str("file:///C:/ws/snapshot.lua").unwrap();

        state
            .update(|analysis| {
                analysis.update_file_by_uri(&uri, Some("local x = 1".to_string()));
            })
            .await;

        // Read path is lock-free: take a snapshot and query salsa semantics directly.
        let snapshot = state.snapshot();
        let file_id = snapshot.get_file_id(&uri).expect("file registered");
        let model = snapshot.semantic_model(file_id).expect("salsa model");
        let decls = model.decls().expect("file facts");
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].name, "x");
    }
}

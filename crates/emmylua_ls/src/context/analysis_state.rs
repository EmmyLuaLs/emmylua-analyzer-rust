//! # AnalysisState — shared analysis state protected by a read-write lock
//!
//! Salsa is gone. Reads are served directly from the live `EmmyLuaAnalysis` under a
//! read lock; writes take the write lock exclusively. We do not clone the whole
//! analysis to run queries.

use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use emmylua_code_analysis::EmmyLuaAnalysis;

use crate::context::RequestOutcome;

pub struct AnalysisState {
    inner: RwLock<EmmyLuaAnalysis>,
}

impl AnalysisState {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(EmmyLuaAnalysis::new()),
        }
    }

    pub fn with_snapshot<T>(&self, f: impl FnOnce(&EmmyLuaAnalysis) -> T) -> Option<T> {
        let analysis = self.read();
        Some(f(&analysis))
    }

    pub fn try_with_snapshot<R>(&self, f: impl FnOnce(&EmmyLuaAnalysis) -> Option<R>) -> Option<R> {
        let analysis = self.read();
        f(&analysis)
    }

    pub fn query<R>(&self, f: impl FnOnce(&EmmyLuaAnalysis) -> Option<R>) -> RequestOutcome<R> {
        let analysis = self.read();
        match f(&analysis) {
            Some(value) => RequestOutcome::Ready(value),
            None => RequestOutcome::Missing,
        }
    }

    pub async fn update<R>(&self, f: impl FnOnce(&mut EmmyLuaAnalysis) -> R) -> R {
        let mut analysis = self.write();
        f(&mut analysis)
    }

    fn read(&self) -> RwLockReadGuard<'_, EmmyLuaAnalysis> {
        self.inner.read().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write(&self) -> RwLockWriteGuard<'_, EmmyLuaAnalysis> {
        self.inner.write().unwrap_or_else(|poisoned| poisoned.into_inner())
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
    async fn update_broadcasts_analysis_state() {
        let state = AnalysisState::new();
        let uri = lsp_types::Uri::from_str("file:///C:/ws/snapshot.lua").unwrap();

        state
            .update(|analysis| {
                analysis.update_file_by_uri(&uri, Some("local x = 1".to_string()));
            })
            .await;

        // Read path uses the live analysis under a read lock.
        let result = state.try_with_snapshot(|analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            let model = analysis.semantic_model(file_id)?;
            let decls = model.decls()?;
            Some((decls.len(), decls[0].name.as_str().to_string()))
        });
        assert_eq!(result, Some((1, "x".to_string())));
    }
}

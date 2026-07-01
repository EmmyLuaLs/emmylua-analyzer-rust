#[derive(Debug, Clone)]
pub struct CacheOptions {
    pub analysis_phase: LuaAnalysisPhase,
}

impl Default for CacheOptions {
    fn default() -> Self {
        Self {
            analysis_phase: LuaAnalysisPhase::Ordered,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LuaAnalysisPhase {
    // Ordered phase
    Ordered,
    // Unordered phase
    Unordered,
    // Force analysis phase
    Force,
}

impl LuaAnalysisPhase {
    pub fn is_ordered(&self) -> bool {
        matches!(self, LuaAnalysisPhase::Ordered)
    }

    pub fn is_unordered(&self) -> bool {
        matches!(self, LuaAnalysisPhase::Unordered)
    }

    pub fn is_force(&self) -> bool {
        matches!(self, LuaAnalysisPhase::Force)
    }
}

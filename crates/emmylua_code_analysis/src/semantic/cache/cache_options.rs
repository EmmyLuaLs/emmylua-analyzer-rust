#[derive(Debug, Clone, Copy)]
pub struct CacheOptions {
    pub analysis_phase: LuaAnalysisPhase,
    // Development/test escape hatch: keep flow inference unbounded so budget
    // blowups can be reproduced instead of hidden behind `unknown`.
    pub disable_flow_inference_step_budget: bool,
}

impl Default for CacheOptions {
    fn default() -> Self {
        Self {
            analysis_phase: LuaAnalysisPhase::Ordered,
            disable_flow_inference_step_budget: false,
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

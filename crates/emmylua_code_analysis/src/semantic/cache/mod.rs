mod cache_options;

pub use cache_options::{CacheOptions, LuaAnalysisPhase};
use emmylua_parser::{LuaExpr, LuaSyntaxId, LuaVarExpr};
use hashbrown::{HashMap, HashSet};
use std::{mem, rc::Rc, sync::Arc};

use crate::{
    FileId, FlowId, LuaFunctionType,
    db_index::LuaType,
    semantic::infer::{ConditionFlowAction, InferConditionFlow, VarRefId},
};

#[derive(Debug)]
pub enum CacheEntry<T> {
    Ready,
    Cache(T),
}

#[derive(Debug, Clone)]
pub(in crate::semantic) struct FlowAssignmentInfo {
    pub vars: Vec<LuaVarExpr>,
    pub exprs: Vec<LuaExpr>,
    pub var_ref_ids: Vec<Option<VarRefId>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::semantic) enum FlowMode {
    WithConditions,
    WithoutConditions,
}

impl FlowMode {
    pub fn uses_conditions(self) -> bool {
        matches!(self, Self::WithConditions)
    }
}

#[derive(Debug, Default)]
pub(in crate::semantic) struct FlowVarCache {
    pub type_cache: HashMap<(FlowId, FlowMode), CacheEntry<LuaType>>,
    pub condition_cache: HashMap<(FlowId, InferConditionFlow), CacheEntry<ConditionFlowAction>>,
}

#[derive(Debug)]
pub struct LuaInferCache {
    file_id: FileId,
    config: CacheOptions,
    no_flow_mode: bool,
    pub expr_cache: HashMap<LuaSyntaxId, CacheEntry<LuaType>>,
    pub(in crate::semantic) expr_no_flow_cache: HashMap<LuaSyntaxId, CacheEntry<Option<LuaType>>>,
    pub call_cache:
        HashMap<(LuaSyntaxId, Option<usize>, LuaType), CacheEntry<Arc<LuaFunctionType>>>,
    pub(in crate::semantic) call_no_flow_cache:
        HashMap<(LuaSyntaxId, Option<usize>, LuaType), CacheEntry<Option<Arc<LuaFunctionType>>>>,
    replay_expr_types: Vec<(LuaSyntaxId, LuaType)>,
    pub(in crate::semantic) flow_cache_var_ref_ids: HashMap<VarRefId, u32>,
    pub(in crate::semantic) next_flow_cache_var_ref_id: u32,
    pub(in crate::semantic) flow_var_caches: Vec<FlowVarCache>,
    pub(in crate::semantic) flow_branch_inputs_cache: Vec<Option<Arc<[FlowId]>>>,
    pub(in crate::semantic) flow_assignment_info_cache: Vec<Option<Rc<FlowAssignmentInfo>>>,
    pub(in crate::semantic) no_flow_table_exprs: HashSet<LuaSyntaxId>,
    pub index_ref_origin_type_cache: HashMap<VarRefId, CacheEntry<LuaType>>,
    pub expr_var_ref_id_cache: HashMap<LuaSyntaxId, VarRefId>,
}

impl LuaInferCache {
    pub fn new(file_id: FileId, config: CacheOptions) -> Self {
        Self {
            file_id,
            config,
            no_flow_mode: false,
            expr_cache: HashMap::new(),
            expr_no_flow_cache: HashMap::new(),
            call_cache: HashMap::new(),
            call_no_flow_cache: HashMap::new(),
            replay_expr_types: Vec::new(),
            flow_cache_var_ref_ids: HashMap::new(),
            next_flow_cache_var_ref_id: 0,
            flow_var_caches: Vec::new(),
            flow_branch_inputs_cache: Vec::new(),
            flow_assignment_info_cache: Vec::new(),
            no_flow_table_exprs: HashSet::new(),
            index_ref_origin_type_cache: HashMap::new(),
            expr_var_ref_id_cache: HashMap::new(),
        }
    }

    pub fn get_config(&self) -> &CacheOptions {
        &self.config
    }

    pub fn get_file_id(&self) -> FileId {
        self.file_id
    }

    pub(in crate::semantic) fn is_no_flow(&self) -> bool {
        self.no_flow_mode
    }

    pub(in crate::semantic) fn with_no_flow<R>(
        &mut self,
        f: impl FnOnce(&mut LuaInferCache) -> R,
    ) -> R {
        let previous_mode = mem::replace(&mut self.no_flow_mode, true);
        let result = f(self);
        self.no_flow_mode = previous_mode;
        result
    }

    pub(in crate::semantic) fn replay_expr_type(&self, syntax_id: LuaSyntaxId) -> Option<&LuaType> {
        self.replay_expr_types
            .iter()
            .rev()
            .find_map(|(overlay_id, ty)| (*overlay_id == syntax_id).then_some(ty))
    }

    pub(in crate::semantic) fn with_replay_overlay<R>(
        &mut self,
        expr_types: &[(LuaSyntaxId, LuaType)],
        table_exprs: &[LuaSyntaxId],
        f: impl FnOnce(&mut LuaInferCache) -> R,
    ) -> R {
        if expr_types.is_empty() && table_exprs.is_empty() {
            return f(self);
        }

        // Replay overlays change no-flow answers, so isolate overlay-dependent
        // cache writes from the normal no-flow caches.
        let overlay_len = self.replay_expr_types.len();
        self.replay_expr_types.extend(expr_types.iter().cloned());
        let mut inserted_table_exprs = Vec::new();
        for syntax_id in table_exprs {
            if self.no_flow_table_exprs.insert(*syntax_id) {
                inserted_table_exprs.push(*syntax_id);
            }
        }
        let saved_expr_no_flow_cache = mem::take(&mut self.expr_no_flow_cache);
        let saved_call_no_flow_cache = mem::take(&mut self.call_no_flow_cache);

        let result = f(self);

        self.expr_no_flow_cache = saved_expr_no_flow_cache;
        self.call_no_flow_cache = saved_call_no_flow_cache;
        for syntax_id in inserted_table_exprs {
            self.no_flow_table_exprs.remove(&syntax_id);
        }
        self.replay_expr_types.truncate(overlay_len);
        result
    }

    pub fn set_phase(&mut self, phase: LuaAnalysisPhase) {
        self.config.analysis_phase = phase;
    }

    pub fn clear(&mut self) {
        self.no_flow_mode = false;
        self.expr_cache.clear();
        self.expr_no_flow_cache.clear();
        self.call_cache.clear();
        self.call_no_flow_cache.clear();
        self.replay_expr_types.clear();
        self.flow_cache_var_ref_ids.clear();
        self.next_flow_cache_var_ref_id = 0;
        self.flow_var_caches.clear();
        self.flow_branch_inputs_cache.clear();
        self.flow_assignment_info_cache.clear();
        self.no_flow_table_exprs.clear();
        self.index_ref_origin_type_cache.clear();
        self.expr_var_ref_id_cache.clear();
    }
}

mod check_reason;
mod find_decl_function;
mod resolve;
mod resolve_closure;

use std::collections::HashMap;

use crate::{
    FileId, InferFailReason, LuaMemberFeature, LuaSemanticDeclId,
    compilation::analyzer::{AnalysisPipeline, unresolve::resolve::try_resolve_constructor},
    db_index::{DbIndex, LuaDeclId, LuaMemberId, LuaSignatureId},
    profile::Profile,
};
use check_reason::{check_reach_reason, resolve_all_reason};
use emmylua_parser::{
    LuaAssignStat, LuaCallExpr, LuaExpr, LuaFuncStat, LuaNameToken, LuaTableExpr, LuaTableField,
};
use resolve::{
    try_resolve_decl, try_resolve_iter_var, try_resolve_member, try_resolve_module,
    try_resolve_module_ref, try_resolve_return_point, try_resolve_table_field,
};
use resolve_closure::{
    try_resolve_call_closure_params, try_resolve_closure_parent_params, try_resolve_closure_return,
};

use super::{AnalyzeContext, infer_cache_manager::InferCacheManager, lua::LuaReturnPoint};

type ResolveResult = Result<(), InferFailReason>;

pub struct UnResolveAnalysisPipeline;

impl AnalysisPipeline for UnResolveAnalysisPipeline {
    fn analyze(db: &mut DbIndex, context: &mut AnalyzeContext) {
        let _p = Profile::cond_new("resolve analyze", context.tree_list.len() > 1);
        let mut infer_manager = std::mem::take(&mut context.infer_manager);
        infer_manager.clear();
        let mut reason_resolve: HashMap<InferFailReason, Vec<UnResolve>> = HashMap::new();
        for (unresolve, reason) in context.unresolves.drain(..) {
            reason_resolve
                .entry(reason.clone())
                .or_default()
                .push(unresolve);
        }

        let mut loop_count = 0;
        while !reason_resolve.is_empty() {
            try_resolve(db, &mut infer_manager, &mut reason_resolve);

            if reason_resolve.is_empty() {
                break;
            }

            if loop_count == 0 {
                infer_manager.set_force();
            }

            resolve_all_reason(db, &mut reason_resolve, loop_count);

            if loop_count >= 5 {
                break;
            }
            loop_count += 1;
        }
    }
}

#[allow(unused)]
fn record_unresolve_info(
    time_hash_map: HashMap<usize, (u128, usize)>,
    reason_unresolves: &HashMap<InferFailReason, Vec<UnResolve>>,
) {
    let mut unresolve_info: HashMap<String, usize> = HashMap::new();
    for (check_reason, unresolves) in reason_unresolves.iter() {
        for unresolve in unresolves {
            match unresolve {
                UnResolve::Return(_) => {
                    unresolve_info
                        .entry("UnResolveReturn".to_string())
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                }
                UnResolve::Decl(_) => {
                    unresolve_info
                        .entry("UnResolveDecl".to_string())
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                }
                UnResolve::Member(_) => {
                    unresolve_info
                        .entry("UnResolveMember".to_string())
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                }
                UnResolve::Module(_) => {
                    unresolve_info
                        .entry("UnResolveModule".to_string())
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                }
                UnResolve::ClosureParams(_) => {
                    unresolve_info
                        .entry("UnResolveClosureParams".to_string())
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                }
                UnResolve::ClosureReturn(_) => {
                    unresolve_info
                        .entry("UnResolveClosureReturn".to_string())
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                }
                UnResolve::ClosureParentParams(_) => {
                    unresolve_info
                        .entry("UnResolveClosureParentParams".to_string())
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                }
                UnResolve::IterDecl(_) => {
                    unresolve_info
                        .entry("UnResolveIterDecl".to_string())
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                }
                UnResolve::ModuleRef(_) => {
                    unresolve_info
                        .entry("UnResolveModuleRef".to_string())
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                }
                UnResolve::TableField(_) => {
                    unresolve_info
                        .entry("UnResolveTableField".to_string())
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                }
                _ => {}
            }
        }
    }

    log::info!("unresolve reason count {}", reason_unresolves.len());
    let mut s = String::new();
    let mut unresolve_info_vec = unresolve_info
        .iter()
        .map(|(k, v)| (k.clone(), *v))
        .collect::<Vec<_>>();
    unresolve_info_vec.sort_by(|a, b| a.1.cmp(&b.1).reverse());
    s.clear();
    s.push_str("unresolve info:\n");
    for (name, count) in unresolve_info_vec {
        s.push_str(&format!("{}: {}\n", name, count));
    }
    log::info!("{}", s);
}

fn try_resolve(
    db: &mut DbIndex,
    infer_manager: &mut InferCacheManager,
    reason_reasolve: &mut HashMap<InferFailReason, Vec<UnResolve>>,
) {
    loop {
        let mut changed = false;
        let mut to_be_remove = Vec::new();
        let mut retain_unresolve = Vec::new();
        for (check_reason, unresolves) in reason_reasolve.iter_mut() {
            if !check_reach_reason(db, infer_manager, check_reason).unwrap_or(false) {
                continue;
            }

            for mut unresolve in unresolves.drain(..) {
                let file_id = unresolve.get_file_id().unwrap_or(FileId { id: 0 });
                let cache = infer_manager.get_infer_cache(file_id);
                let resolve_result = match &mut unresolve {
                    UnResolve::Decl(un_resolve_decl) => {
                        try_resolve_decl(db, cache, un_resolve_decl)
                    }
                    UnResolve::Member(un_resolve_member) => {
                        try_resolve_member(db, cache, un_resolve_member)
                    }
                    UnResolve::Module(un_resolve_module) => {
                        try_resolve_module(db, cache, un_resolve_module)
                    }
                    UnResolve::Return(un_resolve_return) => {
                        try_resolve_return_point(db, cache, un_resolve_return)
                    }
                    UnResolve::ClosureParams(un_resolve_closure_params) => {
                        try_resolve_call_closure_params(db, cache, un_resolve_closure_params)
                    }
                    UnResolve::ClosureReturn(un_resolve_closure_return) => {
                        try_resolve_closure_return(db, cache, un_resolve_closure_return)
                    }
                    UnResolve::IterDecl(un_resolve_iter_var) => {
                        try_resolve_iter_var(db, cache, un_resolve_iter_var)
                    }
                    UnResolve::ModuleRef(module_ref) => {
                        try_resolve_module_ref(db, cache, module_ref)
                    }
                    UnResolve::ClosureParentParams(un_resolve_closure_params) => {
                        try_resolve_closure_parent_params(db, cache, un_resolve_closure_params)
                    }
                    UnResolve::TableField(un_resolve_table_field) => {
                        try_resolve_table_field(db, cache, un_resolve_table_field)
                    }
                    UnResolve::ClassCtor(un_resolve_constructor) => {
                        try_resolve_constructor(db, cache, un_resolve_constructor)
                    }
                };

                match resolve_result {
                    Ok(_) => {
                        changed = true;
                    }
                    Err(InferFailReason::None | InferFailReason::RecursiveInfer) => {}
                    Err(InferFailReason::FieldNotFound) => {
                        if !cache.get_config().analysis_phase.is_force() {
                            retain_unresolve.push((unresolve, InferFailReason::FieldNotFound));
                        }
                    }
                    Err(InferFailReason::UnResolveOperatorCall) => {
                        if !cache.get_config().analysis_phase.is_force() {
                            retain_unresolve
                                .push((unresolve, InferFailReason::UnResolveOperatorCall));
                        }
                    }
                    Err(reason) => {
                        if reason != *check_reason {
                            changed = true;
                            retain_unresolve.push((unresolve, reason));
                        }
                    }
                }
            }

            to_be_remove.push(check_reason.clone());
        }

        for reason in to_be_remove {
            reason_reasolve.remove(&reason);
        }

        for (unresolve, reason) in retain_unresolve {
            reason_reasolve
                .entry(reason.clone())
                .or_default()
                .push(unresolve);
        }

        if !changed || reason_reasolve.is_empty() {
            break;
        }
    }
}

#[derive(Debug)]
pub enum UnResolve {
    Decl(Box<UnResolveDecl>),
    IterDecl(Box<UnResolveIterVar>),
    Member(Box<UnResolveMember>),
    Module(Box<UnResolveModule>),
    Return(Box<UnResolveReturn>),
    ClosureParams(Box<UnResolveCallClosureParams>),
    ClosureReturn(Box<UnResolveClosureReturn>),
    ClosureParentParams(Box<UnResolveParentClosureParams>),
    ModuleRef(Box<UnResolveModuleRef>),
    TableField(Box<UnResolveTableField>),
    ClassCtor(Box<UnResolveConstructor>),
}

#[allow(dead_code)]
impl UnResolve {
    pub fn get_file_id(&self) -> Option<FileId> {
        match self {
            UnResolve::Decl(un_resolve_decl) => Some(un_resolve_decl.file_id),
            UnResolve::IterDecl(un_resolve_iter_var) => Some(un_resolve_iter_var.file_id),
            UnResolve::Member(un_resolve_member) => Some(un_resolve_member.file_id),
            UnResolve::Module(un_resolve_module) => Some(un_resolve_module.file_id),
            UnResolve::Return(un_resolve_return) => Some(un_resolve_return.file_id),
            UnResolve::ClosureParams(un_resolve_closure_params) => {
                Some(un_resolve_closure_params.file_id)
            }
            UnResolve::ClosureReturn(un_resolve_closure_return) => {
                Some(un_resolve_closure_return.file_id)
            }
            UnResolve::ClosureParentParams(un_resolve_closure_params) => {
                Some(un_resolve_closure_params.file_id)
            }
            UnResolve::TableField(un_resolve_table_field) => Some(un_resolve_table_field.file_id),
            UnResolve::ModuleRef(_) => None,
            UnResolve::ClassCtor(un_resolve_constructor) => Some(un_resolve_constructor.file_id),
        }
    }
}

#[derive(Debug)]
pub struct UnResolveDecl {
    pub file_id: FileId,
    pub decl_id: LuaDeclId,
    pub expr: LuaExpr,
    pub ret_idx: usize,
}

impl From<UnResolveDecl> for UnResolve {
    fn from(un_resolve_decl: UnResolveDecl) -> Self {
        UnResolve::Decl(Box::new(un_resolve_decl))
    }
}

#[derive(Debug)]
pub struct UnResolveMember {
    pub file_id: FileId,
    pub member_id: LuaMemberId,
    pub expr: Option<LuaExpr>,
    pub prefix: Option<LuaExpr>,
    pub ret_idx: usize,
}

impl From<UnResolveMember> for UnResolve {
    fn from(un_resolve_member: UnResolveMember) -> Self {
        UnResolve::Member(Box::new(un_resolve_member))
    }
}

#[derive(Debug)]
pub struct UnResolveModule {
    pub file_id: FileId,
    pub expr: LuaExpr,
}

impl From<UnResolveModule> for UnResolve {
    fn from(un_resolve_module: UnResolveModule) -> Self {
        UnResolve::Module(Box::new(un_resolve_module))
    }
}

#[derive(Debug)]
pub struct UnResolveReturn {
    pub file_id: FileId,
    pub signature_id: LuaSignatureId,
    pub return_points: Vec<LuaReturnPoint>,
}

impl From<UnResolveReturn> for UnResolve {
    fn from(un_resolve_return: UnResolveReturn) -> Self {
        UnResolve::Return(Box::new(un_resolve_return))
    }
}

#[derive(Debug)]
pub struct UnResolveCallClosureParams {
    pub file_id: FileId,
    pub signature_id: LuaSignatureId,
    pub call_expr: LuaCallExpr,
    pub param_idx: usize,
}

impl From<UnResolveCallClosureParams> for UnResolve {
    fn from(un_resolve_closure_params: UnResolveCallClosureParams) -> Self {
        UnResolve::ClosureParams(Box::new(un_resolve_closure_params))
    }
}

#[derive(Debug)]
pub struct UnResolveIterVar {
    pub file_id: FileId,
    pub iter_exprs: Vec<LuaExpr>,
    pub iter_vars: Vec<LuaNameToken>,
}

impl From<UnResolveIterVar> for UnResolve {
    fn from(un_resolve_iter_var: UnResolveIterVar) -> Self {
        UnResolve::IterDecl(Box::new(un_resolve_iter_var))
    }
}

#[derive(Debug)]
pub struct UnResolveClosureReturn {
    pub file_id: FileId,
    pub signature_id: LuaSignatureId,
    pub call_expr: LuaCallExpr,
    pub param_idx: usize,
    pub return_points: Vec<LuaReturnPoint>,
}

impl From<UnResolveClosureReturn> for UnResolve {
    fn from(un_resolve_closure_return: UnResolveClosureReturn) -> Self {
        UnResolve::ClosureReturn(Box::new(un_resolve_closure_return))
    }
}

#[derive(Debug)]
pub struct UnResolveModuleRef {
    pub owner_id: LuaSemanticDeclId,
    pub module_file_id: FileId,
}

impl From<UnResolveModuleRef> for UnResolve {
    fn from(un_resolve_module_ref: UnResolveModuleRef) -> Self {
        UnResolve::ModuleRef(Box::new(un_resolve_module_ref))
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug)]
pub enum UnResolveParentAst {
    LuaFuncStat(LuaFuncStat),
    LuaTableField(LuaTableField),
    LuaAssignStat(LuaAssignStat),
}

#[derive(Debug)]
pub struct UnResolveParentClosureParams {
    pub file_id: FileId,
    pub signature_id: LuaSignatureId,
    pub parent_ast: UnResolveParentAst,
}

impl From<UnResolveParentClosureParams> for UnResolve {
    fn from(un_resolve_closure_params: UnResolveParentClosureParams) -> Self {
        UnResolve::ClosureParentParams(Box::new(un_resolve_closure_params))
    }
}

#[derive(Debug)]
pub struct UnResolveTableField {
    pub file_id: FileId,
    pub table_expr: LuaTableExpr,
    pub field: LuaTableField,
    pub decl_feature: LuaMemberFeature,
}

impl From<UnResolveTableField> for UnResolve {
    fn from(un_resolve_table_field: UnResolveTableField) -> Self {
        UnResolve::TableField(Box::new(un_resolve_table_field))
    }
}

#[derive(Debug)]
pub struct UnResolveConstructor {
    pub file_id: FileId,
    pub call_expr: LuaCallExpr,
    pub signature_id: LuaSignatureId,
    pub param_idx: usize,
}

impl From<UnResolveConstructor> for UnResolve {
    fn from(un_resolve_constructor: UnResolveConstructor) -> Self {
        UnResolve::ClassCtor(Box::new(un_resolve_constructor))
    }
}

mod infer_manager;
mod merge_type;
mod resolve;
mod resolve_closure;

use crate::{
    db_index::{DbIndex, LuaDeclId, LuaMemberId, LuaSignatureId},
    profile::Profile,
    FileId,
};
use emmylua_parser::{LuaCallExpr, LuaExpr};
use infer_manager::InferManager;
pub use merge_type::{merge_decl_expr_type, merge_member_type};
use resolve::{
    try_resolve_decl, try_resolve_iter_var, try_resolve_member, try_resolve_module,
    try_resolve_return_point,
};
use resolve_closure::{try_resolve_closure_params, try_resolve_closure_return};

use super::{lua::LuaReturnPoint, AnalyzeContext};

pub fn analyze(db: &mut DbIndex, context: &mut AnalyzeContext) {
    let _p = Profile::cond_new("resolve analyze", context.tree_list.len() > 1);
    let mut unresolves = std::mem::take(&mut context.unresolves);
    let mut infer_manager = InferManager::new(context.config.clone());
    while try_resolve(db, &mut infer_manager, &mut unresolves) {
        unresolves.retain(|un_resolve| match un_resolve {
            UnResolve::None => false,
            _ => true,
        });
    }

    // force_resolve(db, &mut unresolves);
}

fn try_resolve(
    db: &mut DbIndex,
    infer_manager: &mut InferManager,
    unresolves: &mut Vec<UnResolve>,
) -> bool {
    let mut changed = false;
    for i in 0..unresolves.len() {
        let un_resolve = &mut unresolves[i];
        let file_id = un_resolve.get_file_id().unwrap_or(FileId { id: 0 });
        let config = infer_manager.get_infer_config(file_id);
        let resolve = match un_resolve {
            UnResolve::Decl(un_resolve_decl) => {
                try_resolve_decl(db, config, un_resolve_decl).unwrap_or(false)
            }
            UnResolve::Member(ref mut un_resolve_member) => {
                try_resolve_member(db, config, un_resolve_member).unwrap_or(false)
            }
            UnResolve::Module(un_resolve_module) => {
                try_resolve_module(db, config, un_resolve_module).unwrap_or(false)
            }
            UnResolve::Return(un_resolve_return) => {
                try_resolve_return_point(db, config, un_resolve_return).unwrap_or(false)
            }
            UnResolve::ClosureParams(un_resolve_closure_params) => {
                try_resolve_closure_params(db, config, un_resolve_closure_params).unwrap_or(false)
            }
            UnResolve::ClosureReturn(un_resolve_closure_return) => {
                try_resolve_closure_return(db, config, un_resolve_closure_return).unwrap_or(false)
            }
            UnResolve::IterDecl(un_resolve_iter_var) => {
                try_resolve_iter_var(db, config, un_resolve_iter_var).unwrap_or(false)
            }
            UnResolve::None => continue,
        };

        if resolve {
            changed = true;
            unresolves[i] = UnResolve::None;
        }
    }

    changed
}

#[derive(Debug)]
pub enum UnResolve {
    None,
    Decl(Box<UnResolveDecl>),
    IterDecl(Box<UnResolveIterVar>),
    Member(Box<UnResolveMember>),
    Module(Box<UnResolveModule>),
    Return(Box<UnResolveReturn>),
    ClosureParams(Box<UnResolveClosureParams>),
    ClosureReturn(Box<UnResolveClosureReturn>),
}

#[allow(dead_code)]
impl UnResolve {
    pub fn is_none(&self) -> bool {
        matches!(self, UnResolve::None)
    }

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
            UnResolve::None => None,
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
    pub expr: LuaExpr,
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
pub struct UnResolveClosureParams {
    pub file_id: FileId,
    pub signature_id: LuaSignatureId,
    pub call_expr: LuaCallExpr,
    pub param_idx: usize,
}

impl From<UnResolveClosureParams> for UnResolve {
    fn from(un_resolve_closure_params: UnResolveClosureParams) -> Self {
        UnResolve::ClosureParams(Box::new(un_resolve_closure_params))
    }
}

#[derive(Debug)]
pub struct UnResolveIterVar {
    pub file_id: FileId,
    pub decl_id: LuaDeclId,
    pub iter_expr: LuaExpr,
    pub ret_idx: usize,
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

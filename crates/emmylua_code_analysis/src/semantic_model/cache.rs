//! Per-`SemanticModel` local cache.
//!
//! A model owns one cache for its lifetime. Recursion state is represented by
//! `CacheEntry::InProgress` so re-entry checks stay O(1).

use hashbrown::HashMap;

use emmylua_parser::{LuaExpr, LuaSyntaxId};
use rowan::TextSize;
use smol_str::SmolStr;

use crate::member_key::LuaMemberKey;
use crate::semantic_db::MemberList;
use crate::semantic_db::def::SemanticId;
use crate::semantic_db::flow::FlowId;
use crate::{FileId, LuaFunctionType, LuaType};

use super::infer::unify::TplBindings;
use super::member::MemberInfo;
use super::{CallSiteAnalysis, ResolvedMember};

#[derive(Debug, Clone)]
pub(crate) enum CacheEntry<T> {
    InProgress,
    Ready(T),
}

#[derive(Default)]
pub(crate) struct SemanticLocalCache {
    pub(crate) expr_type: HashMap<(FileId, LuaSyntaxId), CacheEntry<LuaType>>,
    pub(crate) decl_type: HashMap<(FileId, SemanticId), CacheEntry<Option<LuaType>>>,
    pub(crate) member_type: HashMap<(FileId, SemanticId), CacheEntry<Option<LuaType>>>,
    pub(crate) resolve_member: HashMap<(FileId, LuaSyntaxId), Option<ResolvedMember>>,
    pub(crate) expr_type_at: HashMap<(FileId, LuaSyntaxId, TextSize), LuaType>,
    pub(crate) resolve_name: HashMap<(FileId, TextSize), Option<SemanticId>>,
    pub(crate) resolve_owner_set: HashMap<SemanticId, Vec<SemanticId>>,
    pub(crate) member_type_at: HashMap<(FileId, SemanticId, TextSize), LuaType>,
    pub(crate) flow_decl: HashMap<(FileId, SemanticId, FlowId), LuaType>,
    /// Fast negative/positive cache for `---@return_cast` lookup by call syntax.
    /// Most calls do not have `return_cast`, so caching the `None` result avoids
    /// repeatedly resolving callees and scanning signatures for the same call.
    pub(crate) return_cast:
        HashMap<(FileId, LuaSyntaxId), Option<(LuaExpr, LuaType, Option<LuaType>, bool)>>,
    pub(crate) callable_candidates: HashMap<(FileId, LuaSyntaxId), Vec<LuaFunctionType>>,
    pub(crate) call_site: HashMap<(FileId, LuaSyntaxId), CallSiteAnalysis>,
    pub(crate) call_site_signatures:
        HashMap<(FileId, LuaSyntaxId), Vec<(LuaFunctionType, TplBindings)>>,
    pub(crate) member_infos: HashMap<LuaType, Vec<MemberInfo>>,
    pub(crate) members_of_owner: HashMap<SemanticId, MemberList>,
    pub(crate) members_of_owner_named: HashMap<(SemanticId, SmolStr), MemberList>,
    pub(crate) member_info: HashMap<(LuaType, LuaMemberKey), Option<MemberInfo>>,
    pub(crate) type_check: HashMap<(LuaType, LuaType), bool>,
    pub(crate) callable_functions: HashMap<LuaType, Vec<LuaFunctionType>>,
    pub(crate) alias_targets: HashMap<SemanticId, Option<LuaType>>,
}

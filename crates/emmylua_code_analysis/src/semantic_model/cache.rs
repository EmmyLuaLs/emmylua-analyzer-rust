//! Per-`SemanticModel` local cache.
//!
//! This is intentionally *not* shared across models or salsa snapshots. A
//! `SemanticModel` is a short-lived view: it is created for one file analysis,
//! accumulates local query results, and is discarded when the analysis is done.

use std::collections::HashMap;

use emmylua_parser::LuaSyntaxId;
use rowan::TextSize;

use crate::member_key::LuaMemberKey;
use crate::salsa_builder::def::SemanticId;
use crate::salsa_builder::flow::FlowId;
use crate::{FileId, LuaFunctionType, LuaType};

use super::member::MemberInfo;
use super::{CallSiteAnalysis, ResolvedMember};

#[derive(Default)]
pub(crate) struct SemanticLocalCache {
    pub(crate) expr_type: HashMap<(FileId, LuaSyntaxId), LuaType>,
    pub(crate) decl_type: HashMap<(FileId, SemanticId), Option<LuaType>>,
    pub(crate) member_type: HashMap<(FileId, SemanticId), Option<LuaType>>,
    pub(crate) resolve_member: HashMap<(FileId, LuaSyntaxId), Option<ResolvedMember>>,
    pub(crate) expr_type_at: HashMap<(FileId, LuaSyntaxId, TextSize), LuaType>,
    pub(crate) member_type_at: HashMap<(FileId, SemanticId, TextSize), LuaType>,
    pub(crate) flow_decl: HashMap<(FileId, SemanticId, FlowId), LuaType>,
    pub(crate) callable_candidates: HashMap<(FileId, LuaSyntaxId), Vec<LuaFunctionType>>,
    pub(crate) call_site: HashMap<(FileId, LuaSyntaxId), CallSiteAnalysis>,
    pub(crate) member_infos: HashMap<LuaType, Vec<MemberInfo>>,
    pub(crate) member_info: HashMap<(LuaType, LuaMemberKey), Option<MemberInfo>>,
    pub(crate) type_check: HashMap<(LuaType, LuaType), bool>,
}

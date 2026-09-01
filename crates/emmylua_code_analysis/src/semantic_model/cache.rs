//! Shared high-level semantic cache.
//!
//! This is the first bridge step before full salsa-tracked semantic queries:
//! instead of keeping these caches inside a short-lived `SemanticModel`
//! instance, store them in an `Arc<SemanticCache>` owned by `SalsaDatabase`.
//! Cloned salsa snapshots therefore share the same high-level cache across
//! requests and worker threads.
//!
//! The cache is split into per-file caches (most high-frequency expression /
//! member / flow results) and a global type-keyed cache (member lists and type
//! compatibility). Per-file caches use independent locks so parallel workspace
//! diagnostics on different files do not contend on one global lock.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use emmylua_parser::LuaSyntaxId;
use internment::ArcIntern;
use rowan::TextSize;

use crate::member_key::LuaMemberKey;
use crate::salsa_builder::def::SemanticId;
use crate::salsa_builder::flow::FlowId;
use crate::{FileId, LuaFunctionType, LuaType};

use super::CallSiteAnalysis;
use super::member::MemberInfo;

/// Interned handle for a structurally-equal `LuaType`.
///
/// This is the first step of the type arena: it turns a full structural `LuaType`
/// into a cheap, stable, equality-based handle that can be used as a cache key.
type InternedLuaType = ArcIntern<LuaType>;

#[derive(Default)]
pub(crate) struct SemanticCache {
    files: RwLock<HashMap<FileId, Arc<FileSemanticCache>>>,
    global: RwLock<GlobalSemanticCache>,
}

#[derive(Default)]
struct FileSemanticCache {
    inner: RwLock<FileSemanticCacheInner>,
}

#[derive(Default)]
struct FileSemanticCacheInner {
    member_type: HashMap<(FileId, SemanticId), Option<LuaType>>,
    decl_type: HashMap<(FileId, SemanticId), Option<LuaType>>,
    expr_type_at: HashMap<(FileId, LuaSyntaxId, TextSize), LuaType>,
    member_type_at: HashMap<(FileId, SemanticId, TextSize), LuaType>,
    callable_candidates: HashMap<(FileId, LuaSyntaxId), Vec<LuaFunctionType>>,
    call_site: HashMap<(FileId, LuaSyntaxId), CallSiteAnalysis>,
    flow_decl: HashMap<(FileId, SemanticId, FlowId), LuaType>,
}

#[derive(Default)]
struct GlobalSemanticCache {
    member_infos: HashMap<InternedLuaType, Vec<MemberInfo>>,
    member_info: HashMap<(InternedLuaType, LuaMemberKey), Option<MemberInfo>>,
}

impl SemanticCache {
    pub(crate) fn clear(&self) {
        *self.files.write().expect("semantic cache lock poisoned") = HashMap::new();
        *self.global.write().expect("semantic cache lock poisoned") =
            GlobalSemanticCache::default();
    }

    /// Invalidate the high-frequency cache for one file.
    ///
    /// The global type-keyed cache is also cleared for now: type definitions can
    /// change when a file changes, and keeping those entries would risk stale
    /// member/type-compatibility results.
    pub(crate) fn clear_file(&self, file_id: FileId) {
        self.files
            .write()
            .expect("semantic cache lock poisoned")
            .remove(&file_id);
        *self.global.write().expect("semantic cache lock poisoned") =
            GlobalSemanticCache::default();
    }

    fn file_cache(&self, file_id: FileId) -> Arc<FileSemanticCache> {
        if let Some(cache) = self
            .files
            .read()
            .expect("semantic cache lock poisoned")
            .get(&file_id)
            .cloned()
        {
            return cache;
        }
        let mut files = self.files.write().expect("semantic cache lock poisoned");
        files
            .entry(file_id)
            .or_insert_with(|| Arc::new(FileSemanticCache::default()))
            .clone()
    }

    pub(crate) fn get_member_type(
        &self,
        file_id: FileId,
        member: &SemanticId,
    ) -> Option<Option<LuaType>> {
        self.file_cache(file_id)
            .inner
            .read()
            .expect("semantic cache lock poisoned")
            .member_type
            .get(&(file_id, member.clone()))
            .cloned()
    }

    pub(crate) fn insert_member_type(
        &self,
        file_id: FileId,
        member: SemanticId,
        ty: Option<LuaType>,
    ) {
        self.file_cache(file_id)
            .inner
            .write()
            .expect("semantic cache lock poisoned")
            .member_type
            .insert((file_id, member), ty);
    }

    pub(crate) fn get_decl_type(
        &self,
        file_id: FileId,
        decl: &SemanticId,
    ) -> Option<Option<LuaType>> {
        self.file_cache(file_id)
            .inner
            .read()
            .expect("semantic cache lock poisoned")
            .decl_type
            .get(&(file_id, decl.clone()))
            .cloned()
    }

    pub(crate) fn insert_decl_type(&self, file_id: FileId, decl: SemanticId, ty: Option<LuaType>) {
        self.file_cache(file_id)
            .inner
            .write()
            .expect("semantic cache lock poisoned")
            .decl_type
            .insert((file_id, decl), ty);
    }

    pub(crate) fn get_expr_type_at(
        &self,
        file_id: FileId,
        syntax: LuaSyntaxId,
        offset: TextSize,
    ) -> Option<LuaType> {
        self.file_cache(file_id)
            .inner
            .read()
            .expect("semantic cache lock poisoned")
            .expr_type_at
            .get(&(file_id, syntax, offset))
            .cloned()
    }

    pub(crate) fn insert_expr_type_at(
        &self,
        file_id: FileId,
        syntax: LuaSyntaxId,
        offset: TextSize,
        ty: LuaType,
    ) {
        self.file_cache(file_id)
            .inner
            .write()
            .expect("semantic cache lock poisoned")
            .expr_type_at
            .insert((file_id, syntax, offset), ty);
    }

    pub(crate) fn get_member_type_at(
        &self,
        file_id: FileId,
        member: &SemanticId,
        offset: TextSize,
    ) -> Option<LuaType> {
        self.file_cache(file_id)
            .inner
            .read()
            .expect("semantic cache lock poisoned")
            .member_type_at
            .get(&(file_id, member.clone(), offset))
            .cloned()
    }

    pub(crate) fn insert_member_type_at(
        &self,
        file_id: FileId,
        member: SemanticId,
        offset: TextSize,
        ty: LuaType,
    ) {
        self.file_cache(file_id)
            .inner
            .write()
            .expect("semantic cache lock poisoned")
            .member_type_at
            .insert((file_id, member, offset), ty);
    }

    pub(crate) fn get_member_infos(&self, prefix_type: &LuaType) -> Option<Vec<MemberInfo>> {
        let key = ArcIntern::new(prefix_type.clone());
        self.global
            .read()
            .expect("semantic cache lock poisoned")
            .member_infos
            .get(&key)
            .cloned()
    }

    pub(crate) fn insert_member_infos(&self, prefix_type: LuaType, infos: Vec<MemberInfo>) {
        let key = ArcIntern::new(prefix_type);
        self.global
            .write()
            .expect("semantic cache lock poisoned")
            .member_infos
            .insert(key, infos);
    }

    pub(crate) fn get_member_info(
        &self,
        prefix_type: &LuaType,
        key: &LuaMemberKey,
    ) -> Option<Option<MemberInfo>> {
        let prefix_key = ArcIntern::new(prefix_type.clone());
        self.global
            .read()
            .expect("semantic cache lock poisoned")
            .member_info
            .get(&(prefix_key, key.clone()))
            .cloned()
    }

    pub(crate) fn insert_member_info(
        &self,
        prefix_type: LuaType,
        key: LuaMemberKey,
        info: Option<MemberInfo>,
    ) {
        let prefix_key = ArcIntern::new(prefix_type);
        self.global
            .write()
            .expect("semantic cache lock poisoned")
            .member_info
            .insert((prefix_key, key), info);
    }

    pub(crate) fn get_callable_candidates(
        &self,
        file_id: FileId,
        syntax: LuaSyntaxId,
    ) -> Option<Vec<LuaFunctionType>> {
        self.file_cache(file_id)
            .inner
            .read()
            .expect("semantic cache lock poisoned")
            .callable_candidates
            .get(&(file_id, syntax))
            .cloned()
    }

    pub(crate) fn insert_callable_candidates(
        &self,
        file_id: FileId,
        syntax: LuaSyntaxId,
        candidates: Vec<LuaFunctionType>,
    ) {
        self.file_cache(file_id)
            .inner
            .write()
            .expect("semantic cache lock poisoned")
            .callable_candidates
            .insert((file_id, syntax), candidates);
    }

    pub(crate) fn get_call_site(
        &self,
        file_id: FileId,
        syntax: LuaSyntaxId,
    ) -> Option<CallSiteAnalysis> {
        self.file_cache(file_id)
            .inner
            .read()
            .expect("semantic cache lock poisoned")
            .call_site
            .get(&(file_id, syntax))
            .cloned()
    }

    pub(crate) fn insert_call_site(
        &self,
        file_id: FileId,
        syntax: LuaSyntaxId,
        analysis: CallSiteAnalysis,
    ) {
        self.file_cache(file_id)
            .inner
            .write()
            .expect("semantic cache lock poisoned")
            .call_site
            .insert((file_id, syntax), analysis);
    }

    pub(crate) fn get_flow_decl(
        &self,
        file_id: FileId,
        decl: &SemanticId,
        flow_id: FlowId,
    ) -> Option<LuaType> {
        self.file_cache(file_id)
            .inner
            .read()
            .expect("semantic cache lock poisoned")
            .flow_decl
            .get(&(file_id, decl.clone(), flow_id))
            .cloned()
    }

    pub(crate) fn insert_flow_decl(
        &self,
        file_id: FileId,
        decl: SemanticId,
        flow_id: FlowId,
        ty: LuaType,
    ) {
        self.file_cache(file_id)
            .inner
            .write()
            .expect("semantic cache lock poisoned")
            .flow_decl
            .insert((file_id, decl, flow_id), ty);
    }
}

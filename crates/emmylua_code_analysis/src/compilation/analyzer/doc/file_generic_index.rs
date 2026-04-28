use hashbrown::HashMap;

use rowan::{TextRange, TextSize};

use crate::{GenericParam, GenericTplId, LuaType};

pub trait GenericIndex: std::fmt::Debug {
    fn add_generic_scope(&mut self, ranges: Vec<TextRange>, is_func: bool) -> GenericScopeId;

    fn append_generic_param(&mut self, scope_id: GenericScopeId, param: GenericParam);

    fn append_generic_params(&mut self, scope_id: GenericScopeId, params: Vec<GenericParam>) {
        for param in params {
            self.append_generic_param(scope_id, param);
        }
    }

    fn append_pending_type_param(&mut self, _param: GenericParam) {}

    fn clear_pending_type_params(&mut self) {}

    fn find_generic(
        &self,
        position: TextSize,
        name: &str,
    ) -> Option<(GenericTplId, Option<LuaType>, Option<LuaType>)>;
}

#[derive(Debug, Clone)]
pub struct FileGenericIndex {
    scopes: Vec<FileGenericScope>,
    pending_type_params: Vec<GenericParam>,
}

impl FileGenericIndex {
    pub fn new() -> Self {
        Self {
            scopes: Vec::new(),
            pending_type_params: Vec::new(),
        }
    }

    pub fn add_generic_scope(&mut self, ranges: Vec<TextRange>, is_func: bool) -> GenericScopeId {
        let scope_id = GenericScopeId::new(self.scopes.len());
        let next_tpl_id = self.next_tpl_id(&ranges, is_func);
        self.scopes.push(FileGenericScope::new(ranges, next_tpl_id));
        scope_id
    }

    pub fn append_generic_param(&mut self, scope_id: GenericScopeId, param: GenericParam) {
        if let Some(scope) = self.scopes.get_mut(scope_id.id) {
            scope.insert_param(param);
        }
    }

    pub fn append_generic_params(&mut self, scope_id: GenericScopeId, params: Vec<GenericParam>) {
        for param in params {
            self.append_generic_param(scope_id, param);
        }
    }

    pub fn append_pending_type_param(&mut self, param: GenericParam) {
        self.pending_type_params.push(param);
    }

    pub fn clear_pending_type_params(&mut self) {
        self.pending_type_params.clear();
    }

    fn next_tpl_id(&self, ranges: &[TextRange], is_func: bool) -> GenericTplId {
        let next_index = self.next_index(ranges, is_func).unwrap_or(0) as u32;
        if is_func {
            GenericTplId::Func(next_index)
        } else {
            GenericTplId::Type(next_index)
        }
    }

    fn next_index(&self, ranges: &[TextRange], is_func: bool) -> Option<usize> {
        let position = ranges.first()?.start();
        Some(
            self.scopes
                .iter()
                .filter(|scope| scope.is_func_scope() == is_func && scope.contains(position))
                .map(|scope| scope.params.len())
                .sum(),
        )
    }

    /// Find generic parameter by position and name.
    /// return (GenericTplId, constraint, default)
    pub fn find_generic(
        &self,
        position: TextSize,
        name: &str,
    ) -> Option<(GenericTplId, Option<LuaType>, Option<LuaType>)> {
        for scope in self.scopes.iter().rev() {
            if !scope.contains(position) {
                continue;
            }

            if let Some((id, param)) = scope.params.get(name) {
                return Some((
                    *id,
                    param.type_constraint.clone(),
                    param.default_type.clone(),
                ));
            }
        }

        // 搜索前置类型参数, 例如 ---@alias Pick<T, K extends keyof T>
        self.pending_type_params
            .iter()
            .enumerate()
            .rev()
            .find(|(_, param)| param.name == name)
            .map(|(idx, param)| {
                (
                    GenericTplId::Type(idx as u32),
                    param.type_constraint.clone(),
                    param.default_type.clone(),
                )
            })
    }
}

impl GenericIndex for FileGenericIndex {
    fn add_generic_scope(&mut self, ranges: Vec<TextRange>, is_func: bool) -> GenericScopeId {
        FileGenericIndex::add_generic_scope(self, ranges, is_func)
    }

    fn append_generic_param(&mut self, scope_id: GenericScopeId, param: GenericParam) {
        FileGenericIndex::append_generic_param(self, scope_id, param);
    }

    fn append_generic_params(&mut self, scope_id: GenericScopeId, params: Vec<GenericParam>) {
        FileGenericIndex::append_generic_params(self, scope_id, params);
    }

    fn append_pending_type_param(&mut self, param: GenericParam) {
        FileGenericIndex::append_pending_type_param(self, param);
    }

    fn clear_pending_type_params(&mut self) {
        FileGenericIndex::clear_pending_type_params(self);
    }

    fn find_generic(
        &self,
        position: TextSize,
        name: &str,
    ) -> Option<(GenericTplId, Option<LuaType>, Option<LuaType>)> {
        FileGenericIndex::find_generic(self, position, name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub struct GenericScopeId {
    pub id: usize,
}

impl GenericScopeId {
    fn new(id: usize) -> Self {
        Self { id }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileGenericScope {
    ranges: Vec<TextRange>,
    params: HashMap<String, (GenericTplId, GenericParam)>,
    next_tpl_id: GenericTplId,
}

impl FileGenericScope {
    fn new(ranges: Vec<TextRange>, next_tpl_id: GenericTplId) -> Self {
        Self {
            ranges,
            params: HashMap::new(),
            next_tpl_id,
        }
    }

    fn is_func_scope(&self) -> bool {
        self.next_tpl_id.is_func()
    }

    fn insert_param(&mut self, param: GenericParam) {
        let tpl_id = self.next_tpl_id;
        self.next_tpl_id = self.next_tpl_id.with_idx((tpl_id.get_idx() + 1) as u32);
        self.params.insert(param.name.to_string(), (tpl_id, param));
    }

    fn contains(&self, position: TextSize) -> bool {
        self.ranges.iter().any(|range| range.contains(position))
    }
}

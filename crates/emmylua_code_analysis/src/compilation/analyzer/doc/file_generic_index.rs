use hashbrown::HashMap;

use rowan::{TextRange, TextSize};
use smol_str::SmolStr;
use std::sync::Arc;

use crate::{GenericParam, GenericTpl, GenericTplId, LuaType};

pub trait GenericIndex: std::fmt::Debug {
    fn add_generic_scope(&mut self, ranges: Vec<TextRange>, is_func: bool) -> GenericScopeId;

    fn append_generic_param(&mut self, scope_id: GenericScopeId, param: GenericParam);

    fn append_generic_params(&mut self, scope_id: GenericScopeId, params: Vec<GenericParam>) {
        for param in params {
            self.append_generic_param(scope_id, param);
        }
    }

    fn find_generic(
        &self,
        position: TextSize,
        name: &str,
    ) -> Option<(GenericTplId, Option<LuaType>, Option<LuaType>)>;
}

#[derive(Debug, Clone)]
pub struct FileGenericIndex {
    scopes: Vec<FileGenericScope>,
}

impl FileGenericIndex {
    pub fn new() -> Self {
        Self { scopes: Vec::new() }
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
}

impl GenericIndex for FileGenericIndex {
    fn add_generic_scope(&mut self, ranges: Vec<TextRange>, is_func: bool) -> GenericScopeId {
        let scope_id = GenericScopeId::new(self.scopes.len());
        let next_tpl_id = self.next_tpl_id(&ranges, is_func);
        self.scopes.push(FileGenericScope::new(ranges, next_tpl_id));
        scope_id
    }

    fn append_generic_param(&mut self, scope_id: GenericScopeId, param: GenericParam) {
        if let Some(scope) = self.scopes.get_mut(scope_id.id) {
            scope.insert_param(param);
        }
    }

    fn append_generic_params(&mut self, scope_id: GenericScopeId, params: Vec<GenericParam>) {
        for param in params {
            self.append_generic_param(scope_id, param);
        }
    }

    /// Find generic parameter by position and name.
    /// return (GenericTplId, constraint, default)
    fn find_generic(
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

        None
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

#[derive(Debug, Clone, Default)]
pub struct ConditionalInferIndex {
    scopes: Vec<ConditionalInferScope>,
    next_infer_id: u32,
}

impl ConditionalInferIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enter_scope(&mut self) {
        self.scopes.push(ConditionalInferScope::new());
    }

    pub fn leave_scope(&mut self) -> Option<ConditionalInferScope> {
        self.scopes.pop()
    }

    pub fn set_current_refs_visible(&mut self, visible: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.refs_visible = visible;
        }
    }

    pub fn declare(&mut self, name: &str) -> Option<Arc<GenericTpl>> {
        let scope_idx = self.scopes.len().checked_sub(1)?;
        if let Some(tpl) = self.scopes[scope_idx].bindings.get(name) {
            return Some(tpl.clone());
        }

        let tpl_id = GenericTplId::ConditionalInfer(self.next_infer_id);
        self.next_infer_id += 1;
        let tpl = Arc::new(GenericTpl::new(
            tpl_id,
            SmolStr::new(name).into(),
            None,
            None,
        ));

        let scope = &mut self.scopes[scope_idx];
        scope.bindings.insert(name.to_string(), tpl.clone());
        scope
            .params
            .push(GenericParam::new(SmolStr::new(name), None, None, None));
        Some(tpl)
    }

    pub fn find_ref(&self, name: &str) -> Option<Arc<GenericTpl>> {
        self.scopes
            .iter()
            .rev()
            .filter(|scope| scope.refs_visible)
            .find_map(|scope| scope.bindings.get(name).cloned())
    }
}

#[derive(Debug, Clone)]
pub struct ConditionalInferScope {
    /// 是否允许在当前阶段把普通名字解析为 conditional infer 引用.
    /// condition 阶段只声明 `infer P`, true 分支阶段才允许引用 `P`.
    refs_visible: bool,
    /// 当前 conditional 作用域内的 `infer` 名字到实际模板的绑定.
    /// 同名 `infer P` 会复用同一个 `GenericTplId::ConditionalInfer`.
    bindings: HashMap<String, Arc<GenericTpl>>,
    /// 当前 conditional 声明过的 infer 参数元数据, 保留给 `LuaConditionalType`.
    params: Vec<GenericParam>,
}

impl ConditionalInferScope {
    fn new() -> Self {
        Self {
            refs_visible: false,
            bindings: HashMap::new(),
            params: Vec::new(),
        }
    }

    pub fn into_params(self) -> Vec<GenericParam> {
        self.params
    }
}

use hashbrown::HashMap;

use rowan::{TextRange, TextSize};

use crate::{GenericParam, GenericTplId, LuaType};

#[derive(Debug, Clone)]
pub struct FileGenericIndex {
    generic_params: Vec<TagGenericParams>,
    root_node_ids: Vec<GenericEffectId>,
    effect_nodes: Vec<GenericEffectRangeNode>,
    pending_type_params: Vec<GenericParam>,
}

impl FileGenericIndex {
    pub fn new() -> Self {
        Self {
            generic_params: Vec::new(),
            root_node_ids: Vec::new(),
            effect_nodes: Vec::new(),
            pending_type_params: Vec::new(),
        }
    }

    pub fn add_generic_scope(&mut self, ranges: Vec<TextRange>, is_func: bool) -> GenericParamId {
        let params_index = self.generic_params.len();
        let start = self.get_start(&ranges).unwrap_or(0);
        self.generic_params
            .push(TagGenericParams::new(is_func, start));
        let params_id = GenericParamId::new(params_index);
        let root_node_ids: Vec<_> = self.root_node_ids.clone();
        for range in ranges {
            let mut added = false;
            for effect_id in root_node_ids.iter() {
                if self.try_add_range_to_effect_node(range, params_id, *effect_id) {
                    added = true;
                }
            }

            if !added {
                let child_node = GenericEffectRangeNode {
                    range,
                    params_id,
                    children: Vec::new(),
                };

                let child_node_id = self.effect_nodes.len();
                self.effect_nodes.push(child_node);
                self.root_node_ids.push(GenericEffectId::new(child_node_id));
            }
        }

        params_id
    }

    pub fn append_generic_param(&mut self, scope_id: GenericParamId, param: GenericParam) {
        if let Some(scope) = self.generic_params.get_mut(scope_id.id) {
            scope.insert_param(param);
        }
    }

    pub fn append_generic_params(&mut self, scope_id: GenericParamId, params: Vec<GenericParam>) {
        for param in params {
            self.append_generic_param(scope_id, param);
        }
    }

    pub fn set_param_constraint(
        &mut self,
        scope_id: GenericParamId,
        name: &str,
        constraint: Option<LuaType>,
    ) {
        if let Some(scope) = self.generic_params.get_mut(scope_id.id)
            && let Some((_idx, stored_param)) = scope.params.get_mut(name)
        {
            stored_param.type_constraint = constraint;
        }
    }

    pub fn append_pending_type_param(&mut self, param: GenericParam) {
        self.pending_type_params.push(param);
    }

    pub fn clear_pending_type_params(&mut self) {
        self.pending_type_params.clear();
    }

    fn get_start(&self, ranges: &[TextRange]) -> Option<usize> {
        let params_ids = self.find_generic_params(ranges.first()?.start())?;
        let mut start = 0;
        for params_id in params_ids.iter() {
            if let Some(params) = self.generic_params.get(*params_id) {
                start += params.params.len();
            }
        }
        Some(start)
    }

    fn try_add_range_to_effect_node(
        &mut self,
        range: TextRange,
        id: GenericParamId,
        effect_id: GenericEffectId,
    ) -> bool {
        let effect_node = match self.effect_nodes.get(effect_id.id) {
            Some(node) => node,
            None => return false,
        };

        if effect_node.range.contains_range(range) {
            let children = effect_node.children.clone();
            for child_effect_id in children {
                if self.try_add_range_to_effect_node(range, id, child_effect_id) {
                    return true;
                }
            }

            let child_node = GenericEffectRangeNode {
                range,
                params_id: id,
                children: Vec::new(),
            };

            let child_node_id = self.effect_nodes.len();
            self.effect_nodes.push(child_node);
            let effect_node = match self.effect_nodes.get_mut(effect_id.id) {
                Some(node) => node,
                None => return false,
            };
            effect_node
                .children
                .push(GenericEffectId::new(child_node_id));
            return true;
        }

        false
    }

    /// Find generic parameter by position and name.
    /// return (GenericTplId, constraint)
    pub fn find_generic(
        &self,
        position: TextSize,
        name: &str,
    ) -> Option<(GenericTplId, Option<LuaType>)> {
        if let Some(params_ids) = self.find_generic_params(position) {
            for params_id in params_ids.iter().rev() {
                if let Some(params) = self.generic_params.get(*params_id)
                    && let Some((id, param)) = params.params.get(name)
                {
                    let tpl_id = if params.is_func {
                        GenericTplId::Func(*id as u32)
                    } else {
                        GenericTplId::Type(*id as u32)
                    };
                    return Some((tpl_id, param.type_constraint.clone()));
                }
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
                )
            })
    }

    fn find_generic_params(&self, position: TextSize) -> Option<Vec<usize>> {
        for effect_id in self.root_node_ids.iter() {
            if self
                .effect_nodes
                .get(effect_id.id)?
                .range
                .contains(position)
            {
                let mut result = Vec::new();
                self.try_find_generic_params(position, *effect_id, &mut result);
                return Some(result);
            }
        }

        None
    }

    fn try_find_generic_params(
        &self,
        position: TextSize,
        effect_id: GenericEffectId,
        result: &mut Vec<usize>,
    ) -> Option<()> {
        let effect_node = self.effect_nodes.get(effect_id.id)?;
        result.push(effect_node.params_id.id);
        for child_effect_id in effect_node.children.iter() {
            let child_effect_node = self.effect_nodes.get(child_effect_id.id)?;
            if child_effect_node.range.contains(position) {
                self.try_find_generic_params(position, *child_effect_id, result);
            }
        }

        Some(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub struct GenericParamId {
    pub id: usize,
}

impl GenericParamId {
    fn new(id: usize) -> Self {
        Self { id }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GenericEffectRangeNode {
    range: TextRange,
    params_id: GenericParamId,
    children: Vec<GenericEffectId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
struct GenericEffectId {
    id: usize,
}

impl GenericEffectId {
    fn new(id: usize) -> Self {
        Self { id }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagGenericParams {
    params: HashMap<String, (usize, GenericParam)>,
    is_func: bool,
    next_index: usize,
}

impl TagGenericParams {
    pub fn new(is_func: bool, start: usize) -> Self {
        Self {
            params: HashMap::new(),
            is_func,
            next_index: start,
        }
    }

    fn insert_param(&mut self, param: GenericParam) {
        let current_index = self.next_index;
        self.next_index += 1;
        self.params
            .insert(param.name.to_string(), (current_index, param));
    }
}

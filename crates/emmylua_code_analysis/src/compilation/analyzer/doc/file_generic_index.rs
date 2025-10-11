use std::collections::HashMap;

use rowan::{TextRange, TextSize};

use crate::{GenericParam, GenericTplId};

#[derive(Debug, Clone)]
pub struct FileGenericIndex {
    generic_params: Vec<TagGenericParams>,
    root_node_ids: Vec<GenericEffectId>,
    effect_nodes: Vec<GenericEffectRangeNode>,
}

impl FileGenericIndex {
    pub fn new() -> Self {
        Self {
            generic_params: Vec::new(),
            root_node_ids: Vec::new(),
            effect_nodes: Vec::new(),
        }
    }

    pub fn add_generic_scope(
        &mut self,
        ranges: Vec<TextRange>,
        params: Vec<GenericParam>,
        is_func: bool,
    ) {
        let params_id = self.generic_params.len();
        // 由于我们允许 infer 推断出一个虚拟泛型, 因此需要计算已声明的泛型数量确定其位置
        let start = self.get_start(&ranges).unwrap_or(0);
        self.generic_params
            .push(TagGenericParams::new(params, is_func, start));
        let params_id = GenericParamId::new(params_id);
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
    /// return (GenericTplId, is_variadic)
    pub fn find_generic(&self, position: TextSize, name: &str) -> Option<(GenericTplId, bool)> {
        let params_ids = self.find_generic_params(position)?;

        for params_id in params_ids.iter().rev() {
            if let Some(params) = self.generic_params.get(*params_id)
                && let Some((id, is_variadic)) = params.params.get(name)
            {
                if params.is_func {
                    return Some((GenericTplId::Func(*id as u32), *is_variadic));
                } else {
                    return Some((GenericTplId::Type(*id as u32), *is_variadic));
                }
            }
        }

        None
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
    params: HashMap<String, (usize, bool)>, // bool: is_variadic
    is_func: bool,
}

impl TagGenericParams {
    pub fn new(generic_params: Vec<GenericParam>, is_func: bool, start: usize) -> Self {
        let mut params = HashMap::new();
        for (i, param) in generic_params.into_iter().enumerate() {
            params.insert(param.name.to_string(), (start + i, param.is_variadic));
        }
        Self { params, is_func }
    }
}

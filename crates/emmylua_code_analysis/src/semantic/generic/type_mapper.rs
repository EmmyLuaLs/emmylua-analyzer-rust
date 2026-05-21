use std::{cell::RefCell, rc::Rc};

use hashbrown::HashMap;

use crate::{DbIndex, GenericTplId, LuaType, LuaTypeDeclId, VariadicType};

#[derive(Debug, Clone)]
pub enum TypeMapper {
    Simple {
        source: GenericTplId,
        target: TypeMapperValue,
    },
    Array {
        mappings: Rc<HashMap<GenericTplId, TypeMapperValue>>,
    },
    InferenceFallback {
        indices: Rc<HashMap<GenericTplId, usize>>,
        targets: Rc<RefCell<Vec<TypeMapperValue>>>,
    },
    Merged {
        layers: Rc<[TypeMapper]>,
    },
}

impl TypeMapper {
    pub fn empty() -> Self {
        Self::from_values(Vec::new(), Vec::new())
    }

    pub fn from_values(sources: Vec<GenericTplId>, targets: Vec<TypeMapperValue>) -> Self {
        if sources.len() == 1 {
            return TypeMapper::Simple {
                source: sources[0],
                target: targets
                    .into_iter()
                    .next()
                    .unwrap_or(TypeMapperValue::Type(LuaType::Any)),
            };
        }

        let mut mappings = HashMap::with_capacity(sources.len().min(targets.len()));
        for (source, target) in sources.into_iter().zip(targets) {
            mappings.entry(source).or_insert(target);
        }
        TypeMapper::Array {
            mappings: Rc::new(mappings),
        }
    }

    pub fn from_type_array(type_array: Vec<LuaType>) -> Self {
        let sources = (0..type_array.len())
            .map(|idx| GenericTplId::Type(idx as u32))
            .collect::<Vec<_>>();
        let targets = type_array
            .into_iter()
            .map(TypeMapperValue::type_value)
            .collect();
        Self::from_values(sources, targets)
    }

    pub fn from_uninferred(sources: Vec<GenericTplId>) -> Self {
        let targets = sources
            .iter()
            .map(|_| TypeMapperValue::None)
            .collect::<Vec<_>>();
        Self::from_values(sources, targets)
    }

    pub fn from_alias(
        db: &DbIndex,
        type_array: Vec<LuaType>,
        alias_type_id: &LuaTypeDeclId,
    ) -> Self {
        let params = db.get_type_index().get_generic_params(alias_type_id);
        let sources = type_array
            .iter()
            .enumerate()
            .map(|(i, _)| {
                params
                    .and_then(|params| params.get(i))
                    .and_then(|param| param.tpl_id)
                    .unwrap_or(GenericTplId::Type(i as u32))
            })
            .collect::<Vec<_>>();
        let targets = type_array
            .into_iter()
            .map(TypeMapperValue::type_value)
            .collect();
        Self::from_values(sources, targets)
    }

    fn collect_layers(mapper: TypeMapper, layers: &mut Vec<TypeMapper>) {
        match mapper {
            TypeMapper::Merged {
                layers: nested_layers,
            } => {
                for layer in nested_layers.iter().cloned() {
                    Self::collect_layers(layer, layers);
                }
            }
            other => layers.push(other),
        }
    }

    pub fn from_inference_fallback(
        indices: Rc<HashMap<GenericTplId, usize>>,
        targets: Rc<RefCell<Vec<TypeMapperValue>>>,
    ) -> Self {
        TypeMapper::InferenceFallback { indices, targets }
    }

    pub fn merge(mapper1: Option<TypeMapper>, mapper2: TypeMapper) -> Self {
        let mut layers = Vec::new();
        if let Some(mapper1) = mapper1 {
            Self::collect_layers(mapper1, &mut layers);
        }
        Self::collect_layers(mapper2, &mut layers);
        match layers.len() {
            0 => Self::empty(),
            1 => layers.remove(0),
            _ => TypeMapper::Merged {
                layers: Rc::from(layers.into_boxed_slice()),
            },
        }
    }

    pub fn prepend(source: GenericTplId, target: LuaType, mapper: Option<TypeMapper>) -> Self {
        let unary = TypeMapper::Simple {
            source,
            target: TypeMapperValue::type_value(target),
        };
        match mapper {
            Some(mapper) => Self::merge(Some(unary), mapper),
            None => unary,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeMapperValue {
    None,
    Type(LuaType),
    Params(Vec<(String, Option<LuaType>)>),
    MultiTypes(Vec<LuaType>),
    MultiBase(LuaType),
}

impl TypeMapperValue {
    pub fn type_value(ty: LuaType) -> Self {
        TypeMapperValue::Type(into_ref_type(ty))
    }

    pub fn params_value(params: Vec<(String, Option<LuaType>)>) -> Self {
        TypeMapperValue::Params(
            params
                .into_iter()
                .map(|(name, ty)| (name, ty.map(into_ref_type)))
                .collect(),
        )
    }

    pub fn raw_type(&self) -> Option<LuaType> {
        match self {
            TypeMapperValue::Type(ty) => Some(ty.clone()),
            TypeMapperValue::Params(params) => params
                .first()
                .and_then(|(_, ty)| ty.clone())
                .or(Some(LuaType::Unknown)),
            TypeMapperValue::MultiTypes(types) => {
                Some(LuaType::Variadic(VariadicType::Multi(types.clone()).into()))
            }
            TypeMapperValue::MultiBase(base) => Some(base.clone()),
            TypeMapperValue::None => None,
        }
    }

    fn direct_tpl_id(&self) -> Option<GenericTplId> {
        match self {
            TypeMapperValue::Type(LuaType::TplRef(tpl))
            | TypeMapperValue::Type(LuaType::ConstTplRef(tpl)) => Some(tpl.get_tpl_id()),
            _ => None,
        }
    }
}

pub(in crate::semantic::generic) fn get_mapped_value(
    tpl_id: GenericTplId,
    mapper: &TypeMapper,
) -> Option<TypeMapperValue> {
    match mapper {
        TypeMapper::Simple { source, target } => {
            if *source == tpl_id {
                Some(target.clone())
            } else {
                None
            }
        }
        TypeMapper::Array { mappings } => mappings.get(&tpl_id).cloned(),
        TypeMapper::InferenceFallback { indices, targets } => {
            let index = *indices.get(&tpl_id)?;
            targets.borrow().get(index).cloned()
        }
        TypeMapper::Merged { layers } => {
            let mut current_tpl_id = tpl_id;
            let mut current_value: Option<TypeMapperValue> = None;
            let mut has_direct_value = false;

            for layer in layers.iter() {
                if let Some(value) = get_mapped_value(current_tpl_id, layer) {
                    if let Some(mapped_tpl_id) = value.direct_tpl_id() {
                        current_tpl_id = mapped_tpl_id;
                        current_value = Some(value);
                        has_direct_value = true;
                        continue;
                    }

                    if matches!(value, TypeMapperValue::None) {
                        if has_direct_value {
                            continue;
                        }
                        return Some(TypeMapperValue::None);
                    }

                    return Some(value);
                }
            }

            current_value
        }
    }
}

fn into_ref_type(ty: LuaType) -> LuaType {
    match ty {
        LuaType::Def(type_decl_id) => LuaType::Ref(type_decl_id),
        _ => ty,
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc, sync::Arc};

    use smol_str::SmolStr;

    use super::*;
    use crate::{GenericTpl, GenericTplId, LuaArrayType};

    fn tpl_ref(idx: u32) -> LuaType {
        LuaType::TplRef(Arc::new(GenericTpl::new(
            GenericTplId::Func(idx),
            SmolStr::new(format!("T{}", idx)).into(),
            None,
            None,
        )))
    }

    // 合并后的 mapper 只读直接结果, 不在查询阶段深度展开结构里的模板.
    #[test]
    fn merged_does_not_deep_instantiate_mapped_result() {
        let first = TypeMapper::from_values(
            vec![GenericTplId::Func(0)],
            vec![TypeMapperValue::type_value(LuaType::Array(
                LuaArrayType::from_base_type(tpl_ref(1)).into(),
            ))],
        );
        let second = TypeMapper::from_values(
            vec![GenericTplId::Func(1)],
            vec![TypeMapperValue::type_value(LuaType::String)],
        );
        let mapper = TypeMapper::merge(Some(first), second);

        let mapped =
            get_mapped_value(GenericTplId::Func(0), &mapper).and_then(|value| value.raw_type());

        assert_eq!(
            mapped,
            Some(LuaType::Array(
                LuaArrayType::from_base_type(tpl_ref(1)).into()
            ))
        );
    }

    // 直接的 TplRef 链要继续追到后层 mapper.
    #[test]
    fn merged_resolves_direct_tpl_ref_through_later_mapper() {
        let first = TypeMapper::from_values(
            vec![GenericTplId::Func(0)],
            vec![TypeMapperValue::type_value(tpl_ref(1))],
        );
        let second = TypeMapper::from_values(
            vec![GenericTplId::Func(1)],
            vec![TypeMapperValue::type_value(LuaType::String)],
        );
        let mapper = TypeMapper::merge(Some(first), second);

        let mapped =
            get_mapped_value(GenericTplId::Func(0), &mapper).and_then(|value| value.raw_type());

        assert_eq!(mapped, Some(LuaType::String));
    }

    // 未推断的槽位要保留为 None, 不能和“没有映射”混掉.
    #[test]
    fn inference_fallback_keeps_unresolved_slots_as_none() {
        let mut indices = HashMap::new();
        indices.insert(GenericTplId::Func(0), 0);
        indices.insert(GenericTplId::Func(1), 1);
        let targets = Rc::new(RefCell::new(vec![
            TypeMapperValue::None,
            TypeMapperValue::None,
        ]));
        let mapper = TypeMapper::from_inference_fallback(Rc::new(indices), targets);

        assert_eq!(
            get_mapped_value(GenericTplId::Func(1), &mapper),
            Some(TypeMapperValue::None)
        );
    }

    // 后层显式 None 不能抹掉前层已经建立的 TplRef 链.
    #[test]
    fn merged_keeps_direct_tpl_ref_when_later_mapper_is_explicit_none() {
        let first = TypeMapper::from_values(
            vec![GenericTplId::Func(0)],
            vec![TypeMapperValue::type_value(tpl_ref(1))],
        );
        let second =
            TypeMapper::from_values(vec![GenericTplId::Func(1)], vec![TypeMapperValue::None]);
        let mapper = TypeMapper::merge(Some(first), second);

        let mapped =
            get_mapped_value(GenericTplId::Func(0), &mapper).and_then(|value| value.raw_type());

        assert_eq!(mapped, Some(tpl_ref(1)));
    }

    // 长链别名要能一路追到最终具体类型.
    #[test]
    fn long_mapper_chain_resolves_transitively() {
        let mut mapper = None;
        for idx in 0..64 {
            let source = GenericTplId::Func(idx);
            let target = if idx == 63 {
                LuaType::String
            } else {
                tpl_ref(idx + 1)
            };
            let layer =
                TypeMapper::from_values(vec![source], vec![TypeMapperValue::type_value(target)]);
            mapper = Some(TypeMapper::merge(mapper, layer));
        }

        let mapper = mapper.expect("mapper");
        assert_eq!(
            get_mapped_value(GenericTplId::Func(0), &mapper).and_then(|value| value.raw_type()),
            Some(LuaType::String)
        );
        assert_eq!(
            get_mapped_value(GenericTplId::Func(31), &mapper).and_then(|value| value.raw_type()),
            Some(LuaType::String)
        );
    }
}

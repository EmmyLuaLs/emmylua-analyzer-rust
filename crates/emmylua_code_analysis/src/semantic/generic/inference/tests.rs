use std::sync::Arc;

use hashbrown::HashSet;
use smol_str::SmolStr;

use super::{
    InferenceCandidate, InferenceContext, InferencePriority, InferenceResult, InferenceVariance,
    context::inference_result_to_mapper_value,
};
use crate::{
    CacheOptions, DbIndex, FileId, GenericTpl, GenericTplId, LuaInferCache, LuaType, LuaTypeDeclId,
    TypeOps,
    semantic::generic::{TypeMapperValue, get_mapped_value},
};

#[test]
fn non_fixing_mapper_does_not_lock_later_candidates() {
    let db = DbIndex::new();
    let mut cache = LuaInferCache::new(FileId::VIRTUAL, CacheOptions::default());
    let mut context = InferenceContext::new(&db, &mut cache, None);
    let tpl_id = GenericTplId::Func(0);
    let tpl = Arc::new(GenericTpl::new(
        tpl_id,
        SmolStr::new("T").into(),
        None,
        None,
    ));
    let return_type = LuaType::TplRef(tpl.clone());

    context.prepare_inference_slots(HashSet::from([tpl_id]));

    // non_fixing_mapper 会读取当前推断结果, 但不应把泛型槽位标记为固定.
    context.insert_type(
        tpl_id,
        InferenceCandidate::ordinary(LuaType::String),
        InferenceVariance::Covariant,
        true,
        InferencePriority::Normal,
    );

    let non_fixing = context.non_fixing_mapper(std::iter::once(&tpl), &return_type);
    assert_eq!(
        get_mapped_value(tpl_id, &non_fixing).and_then(|value| value.raw_type()),
        Some(LuaType::String)
    );

    // 如果上面的 mapper 错误地固定了 T, 这里的新候选会被忽略;
    // 正确行为是继续合并协变候选, 得到 string | integer.
    context.insert_type(
        tpl_id,
        InferenceCandidate::ordinary(LuaType::Integer),
        InferenceVariance::Covariant,
        true,
        InferencePriority::Normal,
    );
    let expected = TypeOps::Union.apply(&db, &LuaType::String, &LuaType::Integer);
    let fixing = context.fixing_mapper(std::iter::once(&tpl), &return_type);
    assert_eq!(
        get_mapped_value(tpl_id, &fixing).and_then(|value| value.raw_type()),
        Some(expected.clone())
    );

    // fixing_mapper 才会真正锁定槽位; 锁定后再插入候选不应改变结果.
    context.insert_type(
        tpl_id,
        InferenceCandidate::ordinary(LuaType::Boolean),
        InferenceVariance::Covariant,
        true,
        InferencePriority::Normal,
    );
    let fixed = context.fixing_mapper(std::iter::once(&tpl), &return_type);
    assert_eq!(
        get_mapped_value(tpl_id, &fixed).and_then(|value| value.raw_type()),
        Some(expected)
    );
}

#[test]
fn variadic_params_mapper_normalizes_def_types() {
    let type_id = LuaTypeDeclId::global("Alias");
    let value = inference_result_to_mapper_value(InferenceResult::VariadicParams(vec![(
        "value".to_string(),
        Some(LuaType::Def(type_id.clone())),
    )]));

    assert_eq!(
        value,
        TypeMapperValue::Params(vec![("value".to_string(), Some(LuaType::Ref(type_id)))])
    );
}

//! 成员类型推断 — 给定前缀类型和成员 key，推断成员类型。
//!
//! 核心方法：`InferQuery::infer_member_type(prefix_type, member_key) -> InferResult`

use smol_str::SmolStr;

use crate::compilation::{SalsaDocVisibilityKindSummary, SalsaSummaryDatabase};
use crate::{FileId, LuaMemberKey, LuaType, LuaTypeDeclId};

use super::{InferFailReason, InferQuery, InferResult};

pub(super) fn infer_member_impl(
    infer: &InferQuery,
    db: &SalsaSummaryDatabase,
    prefix_type: &LuaType,
    member_key: &LuaMemberKey,
) -> InferResult {
    match prefix_type {
        // 宽类型 → 成员可以是任何东西
        LuaType::Table | LuaType::Any | LuaType::Unknown | LuaType::Global => Ok(LuaType::Any),

        LuaType::Nil => Ok(LuaType::Never),

        // 内建类型成员（string, io 等）
        LuaType::String
        | LuaType::Io
        | LuaType::StringConst(_)
        | LuaType::DocStringConst(_)
        | LuaType::Language(_) => {
            let builtin_name = match prefix_type {
                LuaType::String | LuaType::StringConst(_)
                | LuaType::DocStringConst(_) | LuaType::Language(_) => "string",
                LuaType::Io => "io",
                _ => return Err(InferFailReason::NotImplemented),
            };
            let type_id = LuaTypeDeclId::global(builtin_name);
            infer_custom_type_member(infer, db, &type_id, member_key)
        }

        // 自定义类型成员
        LuaType::Ref(type_id) | LuaType::Def(type_id) => {
            infer_custom_type_member(infer, db, type_id, member_key)
        }

        // Object 类型 → 精确字段
        LuaType::Object(obj) => {
            obj.get_field(member_key)
                .cloned()
                .ok_or(InferFailReason::FieldNotFound)
        }

        // Array 类型 → 整数索引
        LuaType::Array(arr) => {
            if matches!(member_key, LuaMemberKey::Integer(_) | LuaMemberKey::ExprType(_))
                && is_integer_key(member_key)
            {
                Ok(arr.get_base().clone())
            } else {
                Err(InferFailReason::FieldNotFound)
            }
        }

        // Tuple 类型 → 整数位置
        LuaType::Tuple(tuple) => {
            if let LuaMemberKey::Integer(i) = member_key {
                let idx = if *i > 0 { (*i - 1) as usize } else { 0 };
                tuple.get_type(idx).cloned().ok_or(InferFailReason::FieldNotFound)
            } else {
                Err(InferFailReason::FieldNotFound)
            }
        }

        // Union → 每个分支都查，结果 union
        LuaType::Union(union) => {
            let members = union.into_vec();
            infer_union_member(infer, db, &members, member_key)
        }

        // Intersection → 每个分支都查，结果 intersect
        LuaType::Intersection(intersection) => {
            infer_intersection_member(infer, db, intersection.get_types(), member_key)
        }

        // Generic → 展开 base 后重查
        LuaType::Generic(generic) => {
            infer_generic_member(infer, db, generic, member_key)
        }

        // TableGeneric → 检查 key 匹配
        LuaType::TableGeneric(table_gen) => {
            infer_table_generic_member(infer, db, table_gen, member_key)
        }

        // ModuleRef → 需要解析模块导出（后续 phase）
        LuaType::ModuleRef(_) => Err(InferFailReason::NotImplemented),

        // TplRef → 需要 extend type（后续 phase）
        LuaType::TplRef(_) | LuaType::ConstTplRef(_) => Err(InferFailReason::NotImplemented),

        // Namespace → 子命名空间或类型
        LuaType::Namespace(ns) => {
            match member_key {
                LuaMemberKey::Name(name) => {
                    let full_name = format!("{}.{}", ns, name);
                    infer_namespace_member(db, infer.file_id, &full_name)
                }
                _ => Err(InferFailReason::FieldNotFound),
            }
        }

        LuaType::Instance(inst) => {
            infer_member_impl(infer, db, inst.get_base(), member_key)
        }

        _ => Err(InferFailReason::FieldNotFound),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 自定义类型成员
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn infer_custom_type_member(
    infer: &InferQuery,
    db: &SalsaSummaryDatabase,
    type_id: &LuaTypeDeclId,
    member_key: &LuaMemberKey,
) -> InferResult {
    let type_name = type_id.get_name();

    // 1. 检查是否为 alias → 展开后重查
    if let Some(origin_type) = resolve_alias_origin(db, infer.file_id, type_name) {
        return infer_member_impl(infer, db, &origin_type, member_key);
    }

    // 2. 通过 salsa 属性查找
    if let Some(ty) = lookup_property_member(db, infer.file_id, type_name, member_key) {
        return Ok(ty);
    }

    // 3. 通过 salsa 成员类型查询（构造 member target）
    if let Some(ty) = lookup_salsa_member_target(db, infer.file_id, type_name, member_key) {
        return Ok(ty);
    }

    // 4. 查找 super types（salsa type_def → super_type_offsets）
    if let Some(super_types) = resolve_super_types(db, infer.file_id, type_name) {
        for super_ty in super_types {
            match infer_member_impl(infer, db, &super_ty, member_key) {
                Ok(member_ty) => return Ok(member_ty),
                Err(InferFailReason::FieldNotFound) => continue,
                Err(err) => return Err(err),
            }
        }
    }

    Err(InferFailReason::FieldNotFound)
}

/// 通过 salsa properties_for_type 查找属性成员。
fn lookup_property_member(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    type_name: &str,
    member_key: &LuaMemberKey,
) -> Option<LuaType> {
    let properties = db.file().properties_for_type(file_id, type_name.into())?;
    for prop in properties {
        let key_matches = match (&prop.key, member_key) {
            (crate::compilation::SalsaPropertyKeySummary::Name(n), LuaMemberKey::Name(k)) => {
                n == k
            }
            (crate::compilation::SalsaPropertyKeySummary::Integer(n), LuaMemberKey::Integer(k)) => {
                n == k
            }
            _ => false,
        };
        if !key_matches {
            continue;
        }
        // property 有 doc_type_offset → 需要 doc type 展开（后续 phase）
        // 当前返回占位
        if prop.doc_type_offset.is_some() {
            return Some(LuaType::Unknown);
        }
    }
    None
}

/// 通过构造 salsa member target 查找成员类型。
fn lookup_salsa_member_target(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    type_name: &str,
    member_key: &LuaMemberKey,
) -> Option<LuaType> {
    let member_name = member_key.get_name()?;
    let target = crate::compilation::SalsaMemberTargetSummary {
        root: crate::compilation::SalsaMemberRootSummary::LocalDecl {
            name: type_name.into(),
            decl_id: crate::compilation::SalsaDeclId(0u32.into()),
        },
        owner_segments: Vec::<SmolStr>::new().into(),
        member_name: SmolStr::new(member_name),
    };
    let member_info = db.types().member(file_id, target)?;
    let candidate = member_info.candidates.first()?;

    if !candidate.named_type_names.is_empty() {
        let types: Vec<LuaType> = candidate
            .named_type_names
            .iter()
            .filter_map(|name| resolve_named_type_for_member(db, file_id, name))
            .collect();
        match types.len() {
            0 => None,
            1 => types.into_iter().next(),
            _ => Some(LuaType::Union(
                crate::LuaUnionType::from_vec(types).into(),
            )),
        }
    } else {
        None
    }
}

fn resolve_named_type_for_member(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    name: &SmolStr,
) -> Option<LuaType> {
    match name.as_str() {
        "nil" => Some(LuaType::Nil),
        "any" => Some(LuaType::Any),
        "boolean" => Some(LuaType::Boolean),
        "string" => Some(LuaType::String),
        "number" => Some(LuaType::Number),
        "integer" | "int" => Some(LuaType::Integer),
        "function" => Some(LuaType::Function),
        "table" => Some(LuaType::Table),
        "thread" => Some(LuaType::Thread),
        "userdata" => Some(LuaType::Userdata),
        _ => {
            let type_def = db.doc().type_def_by_name(file_id, name.as_str())?;
            let type_id = if matches!(type_def.visibility, SalsaDocVisibilityKindSummary::Private) {
                LuaTypeDeclId::local(file_id, name.as_str())
            } else {
                LuaTypeDeclId::global(name.as_str())
            };
            Some(LuaType::Ref(type_id))
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Alias 解析
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn resolve_alias_origin(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    type_name: &str,
) -> Option<LuaType> {
    let type_def = db.doc().type_def_by_name(file_id, type_name)?;
    if !matches!(type_def.kind, crate::compilation::SalsaDocTypeDefKindSummary::Alias) {
        return None;
    }
    // alias 的 origin 在 value_type_offset 中
    // 需要 doc type 展开（后续 phase）
    None
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Super type 解析
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn resolve_super_types(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    type_name: &str,
) -> Option<Vec<LuaType>> {
    let type_def = db.doc().type_def_by_name(file_id, type_name)?;
    if type_def.super_type_offsets.is_empty() {
        return None;
    }

    let mut supers = Vec::new();
    for offset in &type_def.super_type_offsets {
        let lowered = db.doc().resolved_type_by_key(file_id, *offset)?;
        match &lowered.lowered.kind {
            crate::compilation::SalsaDocTypeLoweredKind::Name { name } => {
                // 对 super 类型名，解析为 global Ref
                supers.push(LuaType::Ref(LuaTypeDeclId::global(name.as_str())));
            }
            _ => {
                // 复杂 super 类型（Array, Function, etc.）→ 后续 phase
            }
        }
    }

    if supers.is_empty() {
        None
    } else {
        Some(supers)
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 复合类型成员
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn infer_union_member(
    infer: &InferQuery,
    db: &SalsaSummaryDatabase,
    members: &[LuaType],
    member_key: &LuaMemberKey,
) -> InferResult {
    let mut result_types = Vec::new();
    let mut has_missing = false;

    for sub in members {
        match infer_member_impl(infer, db, sub, member_key) {
            Ok(ty) => result_types.push(ty),
            Err(InferFailReason::FieldNotFound) => has_missing = true,
            Err(err) => return Err(err),
        }
    }

    if result_types.is_empty() {
        return Err(InferFailReason::FieldNotFound);
    }

    if has_missing {
        result_types.push(LuaType::Nil);
    }

    Ok(union_all(result_types))
}

fn infer_intersection_member(
    infer: &InferQuery,
    db: &SalsaSummaryDatabase,
    members: &[LuaType],
    member_key: &LuaMemberKey,
) -> InferResult {
    let mut result: Option<LuaType> = None;

    for sub in members {
        match infer_member_impl(infer, db, sub, member_key) {
            Ok(ty) => {
                result = Some(match result {
                    Some(prev) => intersect_types(prev, ty),
                    None => ty,
                });
            }
            Err(InferFailReason::FieldNotFound) => continue,
            Err(err) => return Err(err),
        }
    }

    result.ok_or(InferFailReason::FieldNotFound)
}

fn infer_generic_member(
    infer: &InferQuery,
    db: &SalsaSummaryDatabase,
    generic: &crate::LuaGenericType,
    member_key: &LuaMemberKey,
) -> InferResult {
    // 检查 alias
    let base_ref_id = generic.get_base_type_id_ref();
    if let Some(type_def) = db.doc().type_def_by_name(infer.file_id, base_ref_id.get_name()) {
        if matches!(type_def.kind, crate::compilation::SalsaDocTypeDefKindSummary::Alias) {
            // Alias + generic → 后续 phase 实现完整泛型实例化
            return Err(InferFailReason::NotImplemented);
        }
    }

    // 从 base type 查找成员
    let base_type = generic.get_base_type();
    infer_member_impl(infer, db, &base_type, member_key)
}

fn infer_table_generic_member(
    infer: &InferQuery,
    _db: &SalsaSummaryDatabase,
    table_params: &[LuaType],
    member_key: &LuaMemberKey,
) -> InferResult {
    if table_params.len() != 2 {
        return Err(InferFailReason::NotImplemented);
    }
    let key_type = &table_params[0];
    let value_type = &table_params[1];

    let access_key = member_key_to_type(member_key);
    if infer.check_type_compact(&access_key, key_type).is_ok() {
        Ok(value_type.clone())
    } else {
        Err(InferFailReason::FieldNotFound)
    }
}

fn infer_namespace_member(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    full_name: &str,
) -> InferResult {
    // 尝试作为类型
    if let Some(type_def) = db.doc().type_def_by_name(file_id, full_name) {
        let type_id = if matches!(type_def.visibility, SalsaDocVisibilityKindSummary::Private) {
            LuaTypeDeclId::local(file_id, full_name)
        } else {
            LuaTypeDeclId::global(full_name)
        };
        return Ok(match type_def.kind {
            crate::compilation::SalsaDocTypeDefKindSummary::Alias => LuaType::Def(type_id),
            _ => LuaType::Ref(type_id),
        });
    }

    // 作为子命名空间
    Ok(LuaType::Namespace(SmolStr::new(full_name).into()))
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 工具函数
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn is_integer_key(key: &LuaMemberKey) -> bool {
    matches!(
        key,
        LuaMemberKey::Integer(_)
    ) || matches!(
        key,
        LuaMemberKey::ExprType(ty) if ty.is_integer()
    )
}

fn member_key_to_type(key: &LuaMemberKey) -> LuaType {
    match key {
        LuaMemberKey::Name(name) => LuaType::StringConst(SmolStr::new(name.as_str()).into()),
        LuaMemberKey::Integer(i) => LuaType::IntegerConst(*i),
        LuaMemberKey::ExprType(ty) => ty.clone(),
        LuaMemberKey::None => LuaType::Unknown,
    }
}

fn union_all(types: Vec<LuaType>) -> LuaType {
    let mut unique = Vec::new();
    for ty in types {
        if !unique.contains(&ty) {
            unique.push(ty);
        }
    }
    match unique.len() {
        0 => LuaType::Unknown,
        1 => unique.into_iter().next().expect("len checked"),
        _ => LuaType::Union(crate::LuaUnionType::from_vec(unique).into()),
    }
}

fn intersect_types(left: LuaType, right: LuaType) -> LuaType {
    if left == right {
        return left;
    }
    if left.is_unknown() {
        return right;
    }
    if right.is_unknown() {
        return left;
    }
    LuaType::Intersection(
        crate::LuaIntersectionType::new(vec![left, right]).into(),
    )
}

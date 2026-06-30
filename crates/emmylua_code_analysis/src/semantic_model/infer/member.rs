//! 成员类型推断 — 给定前缀类型和成员 key，推断成员类型。
//!
//! 核心方法：`InferQuery::infer_member_type(prefix_type, member_key) -> InferResult`

use smol_str::SmolStr;

use crate::compilation::{
    SalsaDeclId, SalsaDocTypeDefKindSummary, SalsaDocTypeLoweredKind,
    SalsaDocVisibilityKindSummary, SalsaMemberRootSummary, SalsaMemberTargetSummary,
    SalsaPropertyKeySummary, SalsaSummaryDatabase, SalsaSyntaxIdSummary,
};
use crate::semantic_model::offset_types::DeclPosition;
use crate::{
    FileId, LuaGenericType, LuaIntersectionType, LuaMemberKey, LuaType, LuaTypeDeclId, LuaUnionType,
};

use super::{InferFailReason, InferQuery, InferResult};

pub(super) fn infer_member_impl(
    infer: &InferQuery,
    db: &SalsaSummaryDatabase,
    prefix_type: &LuaType,
    member_key: &LuaMemberKey,
) -> InferResult {
    let sig_query = infer.sig_query();
    infer_member_impl_inner(infer, db, &sig_query, prefix_type, member_key)
}

fn infer_member_impl_inner(
    infer: &InferQuery,
    db: &SalsaSummaryDatabase,
    sig_query: &crate::semantic_model::SigQuery,
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
                LuaType::String
                | LuaType::StringConst(_)
                | LuaType::DocStringConst(_)
                | LuaType::Language(_) => "string",
                LuaType::Io => "io",
                _ => return Err(InferFailReason::NotImplemented),
            };
            let type_id = LuaTypeDeclId::global(builtin_name);
            infer_custom_type_member(infer, db, sig_query, &type_id, member_key)
        }

        // 自定义类型成员
        LuaType::Ref(type_id) | LuaType::Def(type_id) => {
            infer_custom_type_member(infer, db, sig_query, type_id, member_key)
        }

        // Object 类型 → 精确字段 / 类型兼容模糊匹配
        LuaType::Object(obj) => {
            if let Some(ty) = obj.get_field(member_key) {
                return Ok(ty.clone());
            }
            // ExprType key: 检查是否有字段的 key 类型兼容
            if let LuaMemberKey::ExprType(key_ty) = member_key {
                for (mk, _) in obj.get_fields() {
                    let compatible = match mk {
                        LuaMemberKey::Name(_) => key_ty.is_string() || key_ty.is_str_tpl_ref(),
                        LuaMemberKey::Integer(_) => key_ty.is_integer(),
                        _ => false,
                    };
                    if compatible {
                        return Ok(LuaType::Any);
                    }
                }
            }
            Err(InferFailReason::FieldNotFound)
        }

        // Array 类型 → 整数索引
        LuaType::Array(arr) => {
            if matches!(
                member_key,
                LuaMemberKey::Integer(_) | LuaMemberKey::ExprType(_)
            ) && is_integer_key(member_key)
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
                tuple
                    .get_type(idx)
                    .cloned()
                    .ok_or(InferFailReason::FieldNotFound)
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
        LuaType::Generic(generic) => infer_generic_member(infer, db, generic, member_key),

        // TableGeneric → 检查 key 匹配
        LuaType::TableGeneric(table_gen) => {
            infer_table_generic_member(infer, db, table_gen, member_key)
        }

        // ModuleRef → 需要解析模块导出（后续 phase）
        LuaType::ModuleRef(_) => Err(InferFailReason::NotImplemented),

        // TplRef → 需要 extend type（后续 phase）
        LuaType::TplRef(_) => Err(InferFailReason::NotImplemented),

        // Namespace → 子命名空间或类型
        LuaType::Namespace(ns) => match member_key {
            LuaMemberKey::Name(name) => {
                let full_name = format!("{}.{}", ns, name);
                infer_namespace_member(db, infer.file_id, &full_name)
            }
            _ => Err(InferFailReason::FieldNotFound),
        },

        LuaType::Instance(inst) => {
            infer_member_impl_inner(infer, db, &infer.sig_query(), inst.get_base(), member_key)
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
    sig_query: &crate::semantic_model::SigQuery,
    type_id: &LuaTypeDeclId,
    member_key: &LuaMemberKey,
) -> InferResult {
    let type_name = type_id.get_name();

    // 1. 检查是否为 alias → 展开后重查
    if let Some(origin_type) = resolve_alias_origin(db, sig_query, type_name) {
        return infer_member_impl_inner(infer, db, &infer.sig_query(), &origin_type, member_key);
    }

    // 2. ExprType key → 模糊匹配（如 obj[key] where key: string）
    if let LuaMemberKey::ExprType(key_ty) = member_key {
        if key_compatible_with_any_property(db, infer.file_id, type_name, key_ty) {
            return Ok(LuaType::Any);
        }
    }

    // 3. 检查属性是否声明（跨文件 workspace 查询）
    let property_exists = lookup_property_member(db, sig_query, type_name, member_key).is_some()
        || property_exists_in_workspace(db, type_name, member_key);

    // 4. 通过 salsa 属性获取类型
    if property_exists {
        if let Some(ty) = lookup_property_member(db, sig_query, type_name, member_key) {
            if !matches!(ty, LuaType::Unknown) {
                return Ok(ty);
            }
        }
        // 属性存在但类型未定 → 查询语义类型
        if let Some(ty) = lookup_salsa_member_target(db, infer.file_id, type_name, member_key) {
            return Ok(ty);
        }
        return Ok(LuaType::Any); // 属性存在但无法确定类型
    }

    // 5. 查找 super types（salsa type_def → super_type_offsets）
    if let Some(super_types) = resolve_super_types(db, infer.file_id, type_name) {
        for super_ty in super_types {
            match infer_member_impl_inner(infer, db, &infer.sig_query(), &super_ty, member_key) {
                Ok(member_ty) => return Ok(member_ty),
                Err(InferFailReason::FieldNotFound) => continue,
                Err(err) => return Err(err),
            }
        }
    }

    Err(InferFailReason::FieldNotFound)
}

/// 检查属性是否在工作区任意文件中声明。
fn property_exists_in_workspace(
    db: &SalsaSummaryDatabase,
    type_name: &str,
    member_key: &LuaMemberKey,
) -> bool {
    let Some(entries) = db.semantic().properties_of_type(type_name) else {
        return false;
    };
    entries.iter().any(|e| match (&e.key, member_key) {
        (SalsaPropertyKeySummary::Name(n), LuaMemberKey::Name(k)) => n.as_str() == k.as_str(),
        (SalsaPropertyKeySummary::Integer(n), LuaMemberKey::Integer(k)) => n == k,
        _ => false,
    })
}

/// 检查类型是否有任意属性 key 与 expr 类型兼容（用于 `obj[key]` 模糊匹配）。
/// 跨文件查找：遍历所有 synced 文件。
fn key_compatible_with_any_property(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    type_name: &str,
    key_type: &LuaType,
) -> bool {
    let name: SmolStr = type_name.into();
    for fid in db.file_ids() {
        let Some(properties) = db.file().properties_for_type(fid, name.clone()) else {
            continue;
        };
        for prop in &properties {
            let compatible = match &prop.key {
                SalsaPropertyKeySummary::Name(_) => {
                    key_type.is_string() || key_type.is_str_tpl_ref()
                }
                SalsaPropertyKeySummary::Integer(_) => key_type.is_integer(),
                SalsaPropertyKeySummary::Expr(syntax_id) => {
                    resolve_property_key_type(db, file_id, syntax_id).is_some_and(|t| {
                        key_type.is_string() && t.is_string()
                            || key_type.is_integer() && t.is_integer()
                    })
                }
                _ => false,
            };
            if compatible {
                return true;
            }
        }
    }
    false
}

fn resolve_property_key_type(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_id: &SalsaSyntaxIdSummary,
) -> Option<LuaType> {
    let name_info = db.types().name(file_id, syntax_id.start_offset)?;
    let decl_type = name_info.decl_type?;
    if decl_type.named_type_names.is_empty() {
        return None;
    }
    let name = &decl_type.named_type_names[0];
    match name.as_str() {
        "string" => Some(LuaType::String),
        "number" | "integer" | "int" => Some(LuaType::Integer),
        _ => Some(LuaType::Ref(LuaTypeDeclId::global(name))),
    }
}

/// 通过 salsa properties_for_type 查找属性成员。
fn lookup_property_member(
    db: &SalsaSummaryDatabase,
    sig_query: &crate::semantic_model::SigQuery,
    type_name: &str,
    member_key: &LuaMemberKey,
) -> Option<LuaType> {
    let file_id = sig_query.file_id();
    let properties = db.file().properties_for_type(file_id, type_name.into())?;
    for prop in properties {
        let key_matches = match (&prop.key, member_key) {
            (SalsaPropertyKeySummary::Name(n), LuaMemberKey::Name(k)) => n == k,
            (SalsaPropertyKeySummary::Integer(n), LuaMemberKey::Integer(k)) => n == k,
            _ => false,
        };
        if !key_matches {
            continue;
        }
        if let Some(offset) = prop.doc_type_offset {
            if let Some(resolved) = db.doc().resolved_type_by_key(file_id, offset) {
                // 用新的递归解析器处理所有 lowered kind
                if let Some(ty) = sig_query.resolve_lowered(&resolved.lowered) {
                    return Some(ty);
                }
            }
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
    let target = SalsaMemberTargetSummary {
        root: SalsaMemberRootSummary::LocalDecl {
            name: type_name.into(),
            decl_id: SalsaDeclId::new(DeclPosition(rowan::TextSize::from(0u32))),
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
            _ => Some(LuaType::Union(LuaUnionType::from_vec(types).into())),
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
    sig_query: &crate::semantic_model::SigQuery,
    type_name: &str,
) -> Option<LuaType> {
    let file_id = sig_query.file_id();
    let type_def = db.doc().type_def_by_name(file_id, type_name)?;
    if !matches!(type_def.kind, SalsaDocTypeDefKindSummary::Alias) {
        return None;
    }
    // 用 SigQuery::resolve_lowered 解析 alias 的 underlying type
    let offset = type_def.value_type_offset?;
    let resolved = db.doc().resolved_type_by_key(file_id, offset)?;
    sig_query.resolve_lowered(&resolved.lowered)
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
            SalsaDocTypeLoweredKind::Name { name } => {
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
        let sq = &infer.sig_query();
        match infer_member_impl_inner(infer, db, sq, sub, member_key) {
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
        match infer_member_impl_inner(infer, db, &infer.sig_query(), sub, member_key) {
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
    generic: &LuaGenericType,
    member_key: &LuaMemberKey,
) -> InferResult {
    let sig_query = infer.sig_query();
    let base_ref_id = generic.get_base_type_id_ref();

    // 如果是 alias → 解析底层类型，用泛型参数替换后重试
    if let Some(type_def) = db
        .doc()
        .type_def_by_name(infer.file_id, base_ref_id.get_name())
    {
        if matches!(type_def.kind, SalsaDocTypeDefKindSummary::Alias) {
            if let Some(origin_type) = resolve_alias_origin(db, &sig_query, base_ref_id.get_name())
            {
                // 用 generic 的参数替换 origin 中的泛型引用
                // 简化：直接查成员，让 type_check 处理兼容性
                return infer_member_impl_inner(infer, db, &sig_query, &origin_type, member_key);
            }
            // 无法解析 alias → 回退到 base type
        }
    }

    let base_type = generic.get_base_type();
    infer_member_impl_inner(infer, db, &sig_query, &base_type, member_key)
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
            SalsaDocTypeDefKindSummary::Alias => LuaType::Def(type_id),
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
    matches!(key, LuaMemberKey::Integer(_))
        || matches!(
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
        _ => LuaType::Union(LuaUnionType::from_vec(unique).into()),
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
    LuaType::Intersection(LuaIntersectionType::new(vec![left, right]).into())
}

//! 类型检查模块
//!
//! 检查 source 类型是否兼容 compact（目标/期望）类型。
//! 核心入口：`check_type_compact(source, compact) -> TypeCheckResult`

use std::sync::Arc;

use crate::compilation::{
    SalsaDocTypeDefKindSummary, SalsaDocTypeLoweredKind, SalsaSummaryDatabase,
};
use crate::{
    Emmyrc, LuaArrayType, LuaGenericType, LuaIntersectionType, LuaObjectType, LuaTupleType,
    LuaType, LuaTypeDeclId, LuaUnionType, VariadicType,
};
use crate::{FileId, SalsaDocTypeDefSummary, SalsaDocTypeNodeKey, SalsaDocTypeRef};

pub type TypeCheckResult = Result<(), TypeCheckFailReason>;

#[derive(Debug)]
pub enum TypeCheckFailReason {
    DoNotCheck,
    TypeNotMatch,
    TypeRecursion,
    TypeNotMatchWithReason(String),
}

impl TypeCheckFailReason {
    pub fn is_type_not_match(&self) -> bool {
        matches!(
            self,
            TypeCheckFailReason::TypeNotMatch | TypeCheckFailReason::TypeNotMatchWithReason(_)
        )
    }
}

/// 类型检查上下文。持有递归防护和配置引用。
struct CheckContext<'db> {
    emmyrc: Arc<Emmyrc>,
    db: Option<&'db SalsaSummaryDatabase>,
    file_id: FileId,
    depth: usize,
    collect_detail: bool,
}

const MAX_CHECK_DEPTH: usize = 32;

impl<'db> CheckContext<'db> {
    fn new(emmyrc: Arc<Emmyrc>, collect_detail: bool) -> Self {
        Self {
            emmyrc,
            db: None,
            file_id: FileId { id: 0 },
            depth: 0,
            collect_detail,
        }
    }

    fn with_db(
        emmyrc: Arc<Emmyrc>,
        db: &'db SalsaSummaryDatabase,
        file_id: FileId,
        collect_detail: bool,
    ) -> Self {
        Self {
            emmyrc,
            db: Some(db),
            file_id,
            depth: 0,
            collect_detail,
        }
    }

    fn next_level(&self) -> Result<Self, TypeCheckFailReason> {
        if self.depth >= MAX_CHECK_DEPTH {
            return Err(TypeCheckFailReason::TypeRecursion);
        }
        Ok(Self {
            emmyrc: self.emmyrc.clone(),
            db: self.db,
            file_id: self.file_id,
            depth: self.depth + 1,
            collect_detail: self.collect_detail,
        })
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 公共入口
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 检查 source 类型是否兼容 compact 类型（用于赋值、传参、返回等场景）。
pub fn check_type_compact(
    emmyrc: Arc<Emmyrc>,
    source: &LuaType,
    compact: &LuaType,
) -> TypeCheckResult {
    let ctx = CheckContext::new(emmyrc, false);
    check_general(&ctx, source, compact)
}

/// 详细模式（收集类型不匹配的原因）。
pub fn check_type_compact_detail(
    emmyrc: Arc<Emmyrc>,
    source: &LuaType,
    compact: &LuaType,
) -> TypeCheckResult {
    let ctx = CheckContext::new(emmyrc, true);
    check_general(&ctx, source, compact)
}

/// 数据库感知的详细模式（支持类继承链遍历）。
pub fn check_type_compact_with_db(
    emmyrc: Arc<Emmyrc>,
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    source: &LuaType,
    compact: &LuaType,
    collect_detail: bool,
) -> TypeCheckResult {
    let ctx = CheckContext::with_db(emmyrc, db, file_id, collect_detail);
    check_general(&ctx, source, compact)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 核心分发
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn check_general(ctx: &CheckContext, source: &LuaType, compact: &LuaType) -> TypeCheckResult {
    // Any / Unknown 兼容一切
    if compact.is_unknown() || compact.is_any() {
        return Ok(());
    }

    // Unknown / Never → 通过
    if source.is_unknown() || source.is_any() || source.is_never() {
        return Ok(());
    }

    // compact 为 Never → 通过（不可能满足的约束无需检查）
    if compact.is_never() {
        return Ok(());
    }

    // 快速相等检查
    if source == compact {
        return Ok(());
    }

    // 处理 compact 侧的 Union（source 只需匹配其中一个分支）
    if let LuaType::Union(union) = compact {
        for sub in union.into_vec() {
            if check_general(ctx, source, &sub).is_ok() {
                return Ok(());
            }
        }
        return Err(TypeCheckFailReason::TypeNotMatch);
    }

    // 根据 source 类型分发
    check_source_type(ctx, source, compact)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Source 类型分发
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn check_source_type(ctx: &CheckContext, source: &LuaType, compact: &LuaType) -> TypeCheckResult {
    match source {
        LuaType::Nil => {
            if matches!(compact, LuaType::Nil) {
                return Ok(());
            }
        }

        LuaType::Boolean | LuaType::BooleanConst(_) => {
            if compact.is_boolean() {
                return Ok(());
            }
        }

        LuaType::Integer | LuaType::IntegerConst(_) => {
            if matches!(
                compact,
                LuaType::Integer
                    | LuaType::IntegerConst(_)
                    | LuaType::DocIntegerConst(_)
                    | LuaType::Number
            ) {
                return Ok(());
            }
            if let LuaType::Ref(_) | LuaType::Def(_) = compact {
                return check_ref_as_compact(ctx, source, compact);
            }
        }

        LuaType::Number | LuaType::FloatConst(_) => {
            if compact.is_number() || compact.is_integer() {
                return Ok(());
            }
        }

        LuaType::String => {
            if compact.is_string() {
                return Ok(());
            }
            if let LuaType::Ref(_) | LuaType::Def(_) = compact {
                return check_ref_as_compact(ctx, source, compact);
            }
        }

        LuaType::StringConst(_) => {
            if compact.is_string() {
                return Ok(());
            }
            if let LuaType::Ref(_) | LuaType::Def(_) = compact {
                return check_ref_as_compact(ctx, source, compact);
            }
        }

        LuaType::DocStringConst(s) => {
            if let LuaType::StringConst(t) = compact {
                if s == t {
                    return Ok(());
                }
                return Err(TypeCheckFailReason::TypeNotMatch);
            }
            if compact.is_string() {
                if ctx.emmyrc.strict.doc_base_const_match_base_type {
                    return Err(TypeCheckFailReason::TypeNotMatch);
                }
                return Ok(());
            }
            // DocStringConst against Ref/Def: delegate
            if let LuaType::Ref(_) | LuaType::Def(_) = compact {
                return check_ref_as_compact(ctx, source, compact);
            }
        }

        LuaType::DocIntegerConst(i) => {
            if let LuaType::IntegerConst(j) = compact {
                if i == j {
                    return Ok(());
                }
                return Err(TypeCheckFailReason::TypeNotMatch);
            }
            if compact.is_integer() {
                if ctx.emmyrc.strict.doc_base_const_match_base_type {
                    return Err(TypeCheckFailReason::TypeNotMatch);
                }
                return Ok(());
            }
        }

        LuaType::DocBooleanConst(b) => {
            if let LuaType::BooleanConst(t) = compact {
                if b == t {
                    return Ok(());
                }
                return Err(TypeCheckFailReason::TypeNotMatch);
            }
            if compact.is_boolean() {
                return Err(TypeCheckFailReason::TypeNotMatch);
            }
        }

        LuaType::Function => {
            if compact.is_function() {
                return Ok(());
            }
        }

        LuaType::Table | LuaType::TableConst(_) => {
            if matches!(
                compact,
                LuaType::Table
                    | LuaType::TableConst(_)
                    | LuaType::Tuple(_)
                    | LuaType::Array(_)
                    | LuaType::Object(_)
                    | LuaType::Ref(_)
                    | LuaType::TableGeneric(_)
                    | LuaType::Generic(_)
                    | LuaType::Global
                    | LuaType::Userdata
                    | LuaType::Instance(_)
                    | LuaType::Any
            ) {
                return Ok(());
            }
        }

        LuaType::Userdata => {
            if matches!(
                compact,
                LuaType::Userdata | LuaType::Ref(_) | LuaType::Def(_)
            ) {
                return Ok(());
            }
        }

        LuaType::Thread => {
            if matches!(compact, LuaType::Thread) {
                return Ok(());
            }
        }

        LuaType::Io => {
            if matches!(compact, LuaType::Io) {
                return Ok(());
            }
        }

        LuaType::Global => {
            if matches!(compact, LuaType::Global) {
                return Ok(());
            }
        }

        LuaType::TplRef(_) => return Ok(()),

        LuaType::Namespace(source_ns) => {
            if let LuaType::Namespace(compact_ns) = compact {
                if source_ns == compact_ns {
                    return Ok(());
                }
            }
        }

        LuaType::Language(lang) => match compact {
            LuaType::Language(compact_lang) if lang == compact_lang => return Ok(()),
            LuaType::String | LuaType::StringConst(_) | LuaType::DocStringConst(_) => return Ok(()),
            _ => {}
        },

        LuaType::Variadic(source_var) => {
            return check_variadic(ctx, &source_var, compact);
        }

        // ── 复杂类型 ──
        LuaType::Ref(_) | LuaType::Def(_) => {
            return check_ref_source(ctx, source, compact);
        }

        LuaType::Union(source_union) => {
            return check_union_source(ctx, source_union, compact);
        }

        LuaType::Intersection(source_intersect) => {
            return check_intersection_source(ctx, source_intersect, compact);
        }

        LuaType::Array(source_array) => {
            return check_array_source(ctx, source_array, compact);
        }

        LuaType::Object(source_obj) => {
            return check_object_source(ctx, source_obj, compact);
        }

        LuaType::Tuple(source_tuple) => {
            return check_tuple_source(ctx, source_tuple, compact);
        }

        LuaType::Generic(source_generic) => {
            return check_generic_source(ctx, source_generic, compact);
        }

        LuaType::TableGeneric(source_table_gen) => {
            return check_table_generic_source(ctx, &source_table_gen, compact);
        }

        LuaType::Instance(source_inst) => {
            return check_general(ctx, source_inst.get_base(), compact);
        }

        LuaType::ModuleRef(_) => {
            return Err(TypeCheckFailReason::DoNotCheck);
        }

        LuaType::DocFunction(_) | LuaType::Signature(_) => {
            return check_func_source(ctx, source, compact);
        }

        LuaType::StrTplRef(_) => {
            if compact.is_string() {
                return Ok(());
            }
        }

        // 需要 doc type 展开
        LuaType::Call(_)
        | LuaType::MultiLineUnion(_)
        | LuaType::Mapped(_)
        | LuaType::TypeGuard(_)
        | LuaType::Conditional(_) => {
            return Err(TypeCheckFailReason::DoNotCheck);
        }

        _ => {}
    }

    Err(TypeCheckFailReason::TypeNotMatch)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// DB-aware helpers
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Look up a type definition by name across all files.
fn lookup_type_def(ctx: &CheckContext, name: &str) -> Option<(FileId, SalsaDocTypeDefSummary)> {
    let db = ctx.db?;
    // Try current file first
    if let Some(td) = db.doc().type_def_by_name(ctx.file_id, name) {
        return Some((ctx.file_id, td));
    }
    // Try other files
    for fid in db.file_ids() {
        if fid == ctx.file_id {
            continue;
        }
        if let Some(td) = db.doc().type_def_by_name(fid, name) {
            return Some((fid, td));
        }
    }
    None
}

/// Resolve a value_type_offset through the DB to get the underlying lowered type.
/// Returns a LuaType that can be compared directly.
fn resolve_alias_target_type(
    ctx: &CheckContext,
    file_id: FileId,
    key: &SalsaDocTypeNodeKey,
) -> Option<LuaType> {
    let db = ctx.db?;
    let resolved = db.doc().resolved_type_by_key(file_id, *key)?;
    lowered_kind_to_type(db, file_id, &resolved.lowered.kind)
}

/// Convert a lowered type kind to a LuaType (subset, for alias resolution).
fn lowered_kind_to_type(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    kind: &SalsaDocTypeLoweredKind,
) -> Option<LuaType> {
    match kind {
        SalsaDocTypeLoweredKind::Name { name } => {
            Some(LuaType::Ref(LuaTypeDeclId::global(name.as_str())))
        }
        SalsaDocTypeLoweredKind::Literal { text } => {
            if let Ok(i) = text.parse::<i64>() {
                Some(LuaType::DocIntegerConst(i))
            } else {
                Some(LuaType::DocStringConst(
                    smol_str::SmolStr::new(text.as_str()).into(),
                ))
            }
        }
        SalsaDocTypeLoweredKind::Union { item_types } => {
            let types: Vec<LuaType> = item_types
                .iter()
                .filter_map(|ty_ref| resolve_doc_type_ref(db, file_id, ty_ref))
                .collect();
            if types.is_empty() {
                None
            } else {
                Some(LuaType::Union(LuaUnionType::from_vec(types).into()))
            }
        }
        _ => None,
    }
}

/// Resolve a SalsaDocTypeRef into a LuaType.
fn resolve_doc_type_ref(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    ty_ref: &SalsaDocTypeRef,
) -> Option<LuaType> {
    match ty_ref {
        SalsaDocTypeRef::Node(key) => {
            let lowered = db.doc().lowered_type_by_key(file_id, *key)?;
            lowered_kind_to_type(db, file_id, &lowered.kind)
        }
        _ => None,
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 辅助检查
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn check_ref_as_compact(
    ctx: &CheckContext,
    source: &LuaType,
    compact: &LuaType,
) -> TypeCheckResult {
    let type_decl_id = match compact {
        LuaType::Ref(id) | LuaType::Def(id) => id,
        _ => return Err(TypeCheckFailReason::TypeNotMatch),
    };

    // 1. Quick check: source base type == compact type id
    let source_base_id = get_base_type_id(source);
    if source_base_id.as_ref().is_some_and(|id| id == type_decl_id) {
        return Ok(());
    }

    // 2. DB-based resolution: resolve alias, check subtyping
    if let Some(_db) = ctx.db {
        let compact_name = type_decl_id.get_name();

        // Try to resolve compact via type_def lookup
        if let Some((fid, type_def)) = lookup_type_def(ctx, compact_name) {
            match &type_def.kind {
                SalsaDocTypeDefKindSummary::Alias => {
                    if let Some(value_key) = &type_def.value_type_offset {
                        if let Some(resolved_type) = resolve_alias_target_type(ctx, fid, value_key)
                        {
                            return check_general(ctx, source, &resolved_type);
                        }
                    }
                }
                SalsaDocTypeDefKindSummary::Class => {
                    if let Some(ref src_id) = source_base_id {
                        if is_sub_type_of(ctx, src_id, type_decl_id) {
                            return Ok(());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Err(TypeCheckFailReason::TypeNotMatch)
}

fn check_ref_source(ctx: &CheckContext, source: &LuaType, compact: &LuaType) -> TypeCheckResult {
    let source_id = match source {
        LuaType::Ref(id) | LuaType::Def(id) => id,
        _ => return Err(TypeCheckFailReason::TypeNotMatch),
    };

    // compact 也是 Ref/Def → is_sub_type_of
    if let LuaType::Ref(compact_id) | LuaType::Def(compact_id) = compact {
        if is_sub_type_of(ctx, compact_id, source_id) {
            return Ok(());
        }
        return Err(TypeCheckFailReason::TypeNotMatch);
    }

    // compact 是基础类型 → 检查 source 的基类
    if let Some(source_base) = get_base_type_id(source) {
        let compact_base = get_base_type_id(compact);
        if compact_base.as_ref() == Some(source_id) || source_base == *source_id {
            return check_source_type(ctx, &LuaType::Ref(source_base), compact);
        }
    }

    // compact 是 StringConst/DocStringConst → 检查 source 是否可作为 string
    if compact.is_string() {
        if let LuaType::StringConst(_) | LuaType::DocStringConst(_) = compact {
            // source (Ref/Def) against a specific string literal — check alias resolution
            if let Some(base_id) = get_base_type_id(source) {
                if base_id == LuaTypeDeclId::global("string") {
                    return check_source_type(ctx, &LuaType::String, compact);
                }
            }
            // Defer to DoNotCheck: enum / alias-to-string requires deeper resolution
            return Err(TypeCheckFailReason::DoNotCheck);
        }
        // compact is base type String
        if let Some(base_id) = get_base_type_id(source)
            && base_id == LuaTypeDeclId::global("string")
        {
            return Ok(());
        }
    }

    // source 是 Ref/Def (class/enum) → 检查 compact 是否为其基类型
    if compact_is_supertype_of_ref(compact) {
        let source_name = source_id.get_name();
        if let Some((_fid, type_def)) = lookup_type_def(ctx, source_name) {
            if type_def.kind == SalsaDocTypeDefKindSummary::Class
                || type_def.kind == SalsaDocTypeDefKindSummary::Enum
            {
                return Ok(());
            }
        }
    }

    Err(TypeCheckFailReason::TypeNotMatch)
}

fn compact_is_supertype_of_ref(compact: &LuaType) -> bool {
    matches!(
        compact,
        LuaType::Table
            | LuaType::TableConst(_)
            | LuaType::Userdata
            | LuaType::Any
            | LuaType::Unknown
            | LuaType::Global
    )
}

fn check_variadic(
    ctx: &CheckContext,
    source_var: &VariadicType,
    compact: &LuaType,
) -> TypeCheckResult {
    match source_var {
        VariadicType::Base(base) => {
            if let LuaType::Variadic(compact_var) = compact {
                match compact_var.as_ref() {
                    VariadicType::Base(compact_base) => {
                        if base == compact_base {
                            return Ok(());
                        }
                    }
                    VariadicType::Multi(multi) => {
                        for ct in multi {
                            check_source_type(ctx, base, ct)?;
                        }
                        return Ok(());
                    }
                }
            } else {
                check_source_type(ctx, base, compact)?;
            }
        }
        VariadicType::Multi(_) => {}
    }
    Ok(())
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 复杂类型处理
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn check_union_source(
    ctx: &CheckContext,
    source_union: &LuaUnionType,
    compact: &LuaType,
) -> TypeCheckResult {
    for sub in source_union.into_vec() {
        check_general(ctx, &sub, compact)?;
    }
    Ok(())
}

fn check_intersection_source(
    ctx: &CheckContext,
    source_intersect: &LuaIntersectionType,
    compact: &LuaType,
) -> TypeCheckResult {
    for sub in source_intersect.get_types() {
        if check_general(ctx, sub, compact).is_ok() {
            return Ok(());
        }
    }
    Err(TypeCheckFailReason::TypeNotMatch)
}

fn check_array_source(
    ctx: &CheckContext,
    source_array: &LuaArrayType,
    compact: &LuaType,
) -> TypeCheckResult {
    match compact {
        LuaType::Array(compact_array) => {
            check_general(ctx, source_array.get_base(), compact_array.get_base())
        }
        LuaType::Table | LuaType::TableConst(_) | LuaType::TableGeneric(_) => Ok(()),
        LuaType::Ref(_) | LuaType::Def(_) => check_ref_as_compact(ctx, &LuaType::Table, compact),
        _ => Err(TypeCheckFailReason::TypeNotMatch),
    }
}

fn check_object_source(
    ctx: &CheckContext,
    source_obj: &LuaObjectType,
    compact: &LuaType,
) -> TypeCheckResult {
    match compact {
        LuaType::Object(compact_obj) => {
            for (key, source_field_type) in source_obj.get_fields() {
                match compact_obj.get_field(key) {
                    Some(compact_field_type) => {
                        check_general(ctx, source_field_type, compact_field_type)?;
                    }
                    None => {
                        return Err(TypeCheckFailReason::TypeNotMatch);
                    }
                }
            }
            Ok(())
        }
        LuaType::Table | LuaType::TableConst(_) | LuaType::TableGeneric(_) => Ok(()),
        LuaType::Ref(_) | LuaType::Def(_) => check_ref_as_compact(ctx, &LuaType::Table, compact),
        _ => Err(TypeCheckFailReason::TypeNotMatch),
    }
}

fn check_tuple_source(
    ctx: &CheckContext,
    source_tuple: &LuaTupleType,
    compact: &LuaType,
) -> TypeCheckResult {
    match compact {
        LuaType::Tuple(compact_tuple) => {
            let source_types = source_tuple.get_types();
            let compact_types = compact_tuple.get_types();
            let min_len = source_types.len().min(compact_types.len());
            for i in 0..min_len {
                check_general(ctx, &source_types[i], &compact_types[i])?;
            }
            Ok(())
        }
        LuaType::Table | LuaType::TableConst(_) | LuaType::Array(_) | LuaType::TableGeneric(_) => {
            Ok(())
        }
        _ => Err(TypeCheckFailReason::TypeNotMatch),
    }
}

fn check_generic_source(
    ctx: &CheckContext,
    source_generic: &LuaGenericType,
    compact: &LuaType,
) -> TypeCheckResult {
    if let LuaType::Generic(compact_generic) = compact {
        let source_params = source_generic.get_params();
        let compact_params = compact_generic.get_params();
        if source_params.len() == compact_params.len() {
            for (s, c) in source_params.iter().zip(compact_params.iter()) {
                check_general(ctx, s, c)?;
            }
            return Ok(());
        }
    }
    let base = source_generic.get_base_type();
    check_general(ctx, &base, compact)
}

fn check_table_generic_source(
    ctx: &CheckContext,
    source_table_gen: &[LuaType],
    compact: &LuaType,
) -> TypeCheckResult {
    match compact {
        LuaType::TableGeneric(compact_table_gen) => {
            if source_table_gen.len() == 2 && compact_table_gen.len() == 2 {
                check_general(ctx, &source_table_gen[0], &compact_table_gen[0])?;
                check_general(ctx, &source_table_gen[1], &compact_table_gen[1])?;
                return Ok(());
            }
            Err(TypeCheckFailReason::TypeNotMatch)
        }
        LuaType::Table | LuaType::TableConst(_) => Ok(()),
        _ => Err(TypeCheckFailReason::TypeNotMatch),
    }
}

fn check_func_source(_ctx: &CheckContext, _source: &LuaType, compact: &LuaType) -> TypeCheckResult {
    match compact {
        LuaType::Function | LuaType::DocFunction(_) | LuaType::Signature(_) => Ok(()),
        _ => Err(TypeCheckFailReason::TypeNotMatch),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// is_sub_type_of — 类继承关系
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 检查 sub_id 是否为 super_id 的子类型（遍历类继承链）。
pub fn is_sub_type_of(
    ctx: &CheckContext,
    sub_id: &LuaTypeDeclId,
    super_id: &LuaTypeDeclId,
) -> bool {
    is_sub_type_of_impl(ctx, sub_id, super_id, 0)
}

/// 无 DB 的简单子类型检查（仅按名称 / ID 比较，不遍历继承链）。
pub(crate) fn is_sub_type_of_name(sub_id: &LuaTypeDeclId, super_id: &LuaTypeDeclId) -> bool {
    if sub_id == super_id {
        return true;
    }
    sub_id.get_name() == super_id.get_name()
}

fn is_sub_type_of_impl(
    ctx: &CheckContext,
    sub_id: &LuaTypeDeclId,
    super_id: &LuaTypeDeclId,
    depth: usize,
) -> bool {
    if depth > 32 {
        return false;
    }
    if sub_id == super_id {
        return true;
    }
    let sub_name = sub_id.get_name();
    let super_name = super_id.get_name();
    if sub_name == super_name {
        return true;
    }

    if let Some(db) = &ctx.db {
        let find_supers = |fid: FileId| -> Vec<LuaTypeDeclId> {
            if let Some(def) = db.doc().type_def_by_name(fid, sub_name) {
                def.super_type_offsets
                    .iter()
                    .filter_map(|key| {
                        let resolved = db.doc().resolved_type_by_key(fid, *key)?;
                        match &resolved.lowered.kind {
                            SalsaDocTypeLoweredKind::Name { name } => {
                                Some(LuaTypeDeclId::global(name.as_str()))
                            }
                            _ => None,
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            }
        };

        let supers = find_supers(ctx.file_id);
        for super_type_id in &supers {
            if is_sub_type_of_impl(ctx, super_type_id, super_id, depth + 1) {
                return true;
            }
        }
        if supers.is_empty() {
            for fid in db.file_ids() {
                let supers = find_supers(fid);
                for super_type_id in &supers {
                    if is_sub_type_of_impl(ctx, super_type_id, super_id, depth + 1) {
                        return true;
                    }
                }
                if !supers.is_empty() {
                    break;
                }
            }
        }
    }

    false
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 基础类型映射
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn get_base_type_id(typ: &LuaType) -> Option<LuaTypeDeclId> {
    match typ {
        LuaType::Integer | LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) => {
            Some(LuaTypeDeclId::global("integer"))
        }
        LuaType::Number | LuaType::FloatConst(_) => Some(LuaTypeDeclId::global("number")),
        LuaType::Boolean | LuaType::BooleanConst(_) | LuaType::DocBooleanConst(_) => {
            Some(LuaTypeDeclId::global("boolean"))
        }
        LuaType::String | LuaType::StringConst(_) | LuaType::DocStringConst(_) => {
            Some(LuaTypeDeclId::global("string"))
        }
        LuaType::Table
        | LuaType::TableGeneric(_)
        | LuaType::TableConst(_)
        | LuaType::Tuple(_)
        | LuaType::Array(_)
        | LuaType::Object(_) => Some(LuaTypeDeclId::global("table")),
        LuaType::DocFunction(_) | LuaType::Function | LuaType::Signature(_) => {
            Some(LuaTypeDeclId::global("function"))
        }
        LuaType::Thread => Some(LuaTypeDeclId::global("thread")),
        LuaType::Userdata => Some(LuaTypeDeclId::global("userdata")),
        LuaType::Io => Some(LuaTypeDeclId::global("io")),
        LuaType::Nil => Some(LuaTypeDeclId::global("nil")),
        _ => None,
    }
}

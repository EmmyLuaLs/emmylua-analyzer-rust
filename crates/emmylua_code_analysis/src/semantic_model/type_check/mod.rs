//! 类型检查模块
//!
//! 检查 source 类型是否兼容 compact（目标/期望）类型。
//! 核心入口：`check_type_compact(source, compact) -> TypeCheckResult`

use std::sync::Arc;

use crate::{
    Emmyrc, LuaArrayType, LuaGenericType, LuaIntersectionType, LuaObjectType, LuaTupleType,
    LuaType, LuaTypeDeclId, LuaUnionType, VariadicType,
};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 公共类型
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

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
struct CheckContext {
    emmyrc: Arc<Emmyrc>,
    depth: usize,
    collect_detail: bool,
}

const MAX_CHECK_DEPTH: usize = 32;

impl CheckContext {
    fn new(emmyrc: Arc<Emmyrc>, collect_detail: bool) -> Self {
        Self {
            emmyrc,
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

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 核心分发
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn check_general(
    ctx: &CheckContext,
    source: &LuaType,
    compact: &LuaType,
) -> TypeCheckResult {
    // Any / Unknown 兼容一切
    if compact.is_unknown() || compact.is_any() {
        return Ok(());
    }

    // Unknown source 无法确定 → 通过
    if source.is_unknown() || source.is_any() {
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

fn check_source_type(
    ctx: &CheckContext,
    source: &LuaType,
    compact: &LuaType,
) -> TypeCheckResult {
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
            if matches!(compact, LuaType::Userdata | LuaType::Ref(_) | LuaType::Def(_)) {
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

        LuaType::TplRef(_) | LuaType::ConstTplRef(_) => return Ok(()),

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
            // 模块类型 → 需要解析导出类型
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
        LuaType::Call(_) | LuaType::MultiLineUnion(_) | LuaType::Mapped(_)
        | LuaType::TypeGuard(_) | LuaType::Conditional(_) => {
            return Err(TypeCheckFailReason::DoNotCheck);
        }

        _ => {}
    }

    Err(TypeCheckFailReason::TypeNotMatch)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 辅助检查
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn check_ref_as_compact(
    _ctx: &CheckContext,
    source: &LuaType,
    compact: &LuaType,
) -> TypeCheckResult {
    let type_decl_id = match compact {
        LuaType::Ref(id) | LuaType::Def(id) => id,
        _ => return Err(TypeCheckFailReason::TypeNotMatch),
    };

    // 基础类型映射：检查 source 的基础类型是否匹配 compact 的类
    let source_base_id = get_base_type_id(source);
    if let Some(base_id) = source_base_id {
        if base_id == *type_decl_id {
            return Ok(());
        }
    }

    // 完整的 is_sub_type_of 检查 → 后续 phase 实现
    Err(TypeCheckFailReason::TypeNotMatch)
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

fn check_ref_source(
    ctx: &CheckContext,
    source: &LuaType,
    compact: &LuaType,
) -> TypeCheckResult {
    let source_id = match source {
        LuaType::Ref(id) | LuaType::Def(id) => id,
        _ => return Err(TypeCheckFailReason::TypeNotMatch),
    };

    // compact 也是 Ref/Def → is_sub_type_of
    if let LuaType::Ref(compact_id) | LuaType::Def(compact_id) = compact {
        if is_sub_type_of(source_id, compact_id) {
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

    Err(TypeCheckFailReason::TypeNotMatch)
}

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

fn check_func_source(
    _ctx: &CheckContext,
    _source: &LuaType,
    compact: &LuaType,
) -> TypeCheckResult {
    match compact {
        LuaType::Function | LuaType::DocFunction(_) | LuaType::Signature(_) => Ok(()),
        _ => Err(TypeCheckFailReason::TypeNotMatch),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// is_sub_type_of — 类继承关系
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 检查 sub_type_ref_id 是否为 super_type_ref_id 的子类型。
///
/// 当前实现：直接相等 + 基础类型映射。
/// 完整的类层次遍历（通过 salsa super_type_offsets）后续 phase 实现。
pub fn is_sub_type_of(sub_id: &LuaTypeDeclId, super_id: &LuaTypeDeclId) -> bool {
    if sub_id == super_id {
        return true;
    }

    // 基础类型映射：如 IntegerConst(StringConst) → "string"
    // 这里检查 sub_id 是否为基础类型的别名
    let sub_name = sub_id.get_name();
    let super_name = super_id.get_name();

    // 同类型引用
    if sub_name == super_name {
        return true;
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

//! 索引表达式推断 — `a.b`, `a["b"]`, `a:b()`

use emmylua_parser::{LuaAstNode, LuaIndexExpr, LuaIndexKey, LuaIndexMemberExpr, NumberResult};
use smol_str::SmolStr;

use crate::compilation::{SalsaDocVisibilityKindSummary, SalsaSummaryDatabase};
use crate::{FileId, LuaMemberKey, LuaType, LuaTypeDeclId, LuaUnionType};

use super::{InferFailReason, InferQuery, InferResult};

pub(super) fn infer_index_expr(
    infer: &InferQuery,
    index_expr: LuaIndexExpr,
) -> InferResult {
    let prefix_expr = index_expr.get_prefix_expr().ok_or(InferFailReason::None)?;
    let prefix_type = infer.infer_expr(prefix_expr)?;

    let member_expr = LuaIndexMemberExpr::IndexExpr(index_expr);
    dispatch_prefix_type(infer, &prefix_type, &member_expr)
}

fn candidates_to_type(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    names: &[SmolStr],
) -> Option<LuaType> {
    let types: Vec<LuaType> = names
        .iter()
        .filter_map(|name| resolve_named_type(db, file_id, name))
        .collect();
    match types.len() {
        0 => None,
        1 => types.into_iter().next(),
        _ => Some(LuaType::Union(LuaUnionType::from_vec(types).into())),
    }
}

fn resolve_named_type(
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

fn dispatch_prefix_type(
    infer: &InferQuery,
    prefix_type: &LuaType,
    member_expr: &LuaIndexMemberExpr,
) -> InferResult {
    match prefix_type {
        LuaType::Table | LuaType::Any | LuaType::Unknown | LuaType::Global => Ok(LuaType::Any),

        LuaType::Nil => Ok(LuaType::Never),

        LuaType::Object(obj) => {
            let key = member_key_from_expr(infer, member_expr)?;
            obj.get_field(&key)
                .cloned()
                .ok_or(InferFailReason::FieldNotFound)
        }

        LuaType::Array(arr) => {
            let key = member_key_from_expr(infer, member_expr)?;
            if matches!(key, LuaMemberKey::Integer(_)) {
                Ok(arr.get_base().clone())
            } else {
                Err(InferFailReason::FieldNotFound)
            }
        }

        LuaType::Tuple(tuple) => {
            let key = member_key_from_expr(infer, member_expr)?;
            if let LuaMemberKey::Integer(i) = key {
                let idx = if i > 0 { (i - 1) as usize } else { 0 };
                tuple.get_type(idx).cloned().ok_or(InferFailReason::FieldNotFound)
            } else {
                Err(InferFailReason::FieldNotFound)
            }
        }

        // 委托给 infer_member_type 处理
        LuaType::Ref(_)
        | LuaType::Def(_)
        | LuaType::Union(_)
        | LuaType::Intersection(_)
        | LuaType::Generic(_)
        | LuaType::Instance(_)
        | LuaType::TableGeneric(_)
        | LuaType::TplRef(_)
        | LuaType::ModuleRef(_)
        | LuaType::Namespace(_)
        | LuaType::String
        | LuaType::Io
        | LuaType::StringConst(_)
        | LuaType::DocStringConst(_)
        | LuaType::Language(_) => {
            let key = member_key_from_expr(infer, member_expr)?;
            infer.infer_member_type(prefix_type, &key)
        }

        _ => Err(InferFailReason::FieldNotFound),
    }
}

fn member_key_from_expr(
    infer: &InferQuery,
    member_expr: &LuaIndexMemberExpr,
) -> Result<LuaMemberKey, InferFailReason> {
    let index_key = member_expr.get_index_key().ok_or(InferFailReason::None)?;
    match &index_key {
        LuaIndexKey::Name(name) => Ok(LuaMemberKey::Name(
            SmolStr::new(name.get_name_text()),
        )),
        LuaIndexKey::String(s) => Ok(LuaMemberKey::Name(
            SmolStr::new(s.get_value()),
        )),
        LuaIndexKey::Integer(i) => match i.get_number_value() {
            NumberResult::Int(n) => Ok(LuaMemberKey::Integer(n)),
            _ => Err(InferFailReason::FieldNotFound),
        },
        LuaIndexKey::Idx(i) => Ok(LuaMemberKey::Integer(*i as i64)),
        LuaIndexKey::Expr(expr) => {
            let key_type = infer.infer_expr(expr.clone())?;
            Ok(match key_type {
                LuaType::StringConst(s) => LuaMemberKey::Name((*s).clone()),
                LuaType::IntegerConst(i) => LuaMemberKey::Integer(i),
                LuaType::DocStringConst(s) => LuaMemberKey::Name((*s).clone()),
                LuaType::DocIntegerConst(i) => LuaMemberKey::Integer(i),
                _ => LuaMemberKey::ExprType(key_type),
            })
        }
    }
}

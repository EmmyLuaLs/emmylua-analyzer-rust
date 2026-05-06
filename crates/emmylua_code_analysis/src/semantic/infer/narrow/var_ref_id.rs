use std::ops::Deref;

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaExpr, LuaLiteralToken, PathTrait};
use internment::ArcIntern;
use rowan::TextSize;
use smol_str::SmolStr;

use crate::{
    DbIndex, LuaAliasCallKind, LuaDeclId, LuaDeclOrMemberId, LuaInferCache, LuaMemberId,
    LuaMemberKey, LuaType, infer_expr,
    semantic::infer::{
        infer_index::get_index_expr_var_ref_id, infer_name::get_name_expr_var_ref_id,
    },
};

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VarRefId {
    VarRef(LuaDeclId),
    SelfRef(LuaDeclOrMemberId),
    IndexRef(LuaDeclOrMemberId, ArcIntern<SmolStr>),
}

impl VarRefId {
    pub fn get_decl_id_ref(&self) -> Option<LuaDeclId> {
        match self {
            VarRefId::VarRef(decl_id) => Some(*decl_id),
            VarRefId::SelfRef(decl_or_member_id) => decl_or_member_id.as_decl_id(),
            _ => None,
        }
    }

    pub fn get_member_id_ref(&self) -> Option<LuaMemberId> {
        match self {
            VarRefId::SelfRef(decl_or_member_id) => decl_or_member_id.as_member_id(),
            _ => None,
        }
    }

    pub fn get_position(&self) -> TextSize {
        match self {
            VarRefId::VarRef(decl_id) => decl_id.position,
            VarRefId::SelfRef(decl_or_member_id) => decl_or_member_id.get_position(),
            VarRefId::IndexRef(decl_or_member_id, _) => decl_or_member_id.get_position(),
        }
    }

    pub fn start_with(&self, prefix: &VarRefId) -> bool {
        let (decl_or_member_id, path) = match self {
            VarRefId::IndexRef(decl_or_member_id, path) => {
                (decl_or_member_id.clone(), path.clone())
            }
            _ => return false,
        };

        match prefix {
            VarRefId::VarRef(decl_id) => decl_or_member_id.as_decl_id() == Some(*decl_id),
            VarRefId::SelfRef(ref_decl_or_member_id) => *ref_decl_or_member_id == decl_or_member_id,
            VarRefId::IndexRef(ref_decl_or_member_id, prefix_path) => {
                *ref_decl_or_member_id == decl_or_member_id
                    && path.starts_with(prefix_path.deref().as_str())
            }
        }
    }

    // 计算从 prefix 到当前索引引用的相对字段路径。
    // 例如 `target.handle.name` 相对 `target` 得到 `handle.name`，
    // 后续可在已经被判别字段窄化过的 prefix 类型上逐级投影。
    pub fn relative_index_path(&self, prefix: &VarRefId) -> Option<Vec<LuaMemberKey>> {
        let (decl_or_member_id, path) = match self {
            VarRefId::IndexRef(decl_or_member_id, path) => {
                (decl_or_member_id.clone(), path.deref().as_str())
            }
            _ => return None,
        };

        let relative_path = match prefix {
            VarRefId::VarRef(decl_id) if decl_or_member_id.as_decl_id() == Some(*decl_id) => {
                path.split_once('.').map(|(_, rest)| rest).unwrap_or("")
            }
            VarRefId::SelfRef(ref_decl_or_member_id)
                if *ref_decl_or_member_id == decl_or_member_id =>
            {
                path.split_once('.').map(|(_, rest)| rest).unwrap_or("")
            }
            VarRefId::IndexRef(ref_decl_or_member_id, prefix_path)
                if *ref_decl_or_member_id == decl_or_member_id =>
            {
                let prefix_path = prefix_path.deref().as_str();
                if prefix_path.is_empty() {
                    path
                } else {
                    path.strip_prefix(prefix_path)?.strip_prefix('.')?
                }
            }
            _ => return None,
        };

        if relative_path.is_empty() {
            return Some(Vec::new());
        }

        relative_path
            .split('.')
            .map(|segment| {
                if segment.is_empty() || segment.starts_with('[') {
                    return None;
                }

                if let Ok(index) = segment.parse::<i64>() {
                    return Some(LuaMemberKey::Integer(index));
                }

                Some(LuaMemberKey::Name(segment.into()))
            })
            .collect()
    }

    pub fn is_self_ref(&self) -> bool {
        matches!(self, VarRefId::SelfRef(_))
    }
}

fn get_call_expr_var_ref_id(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    call_expr: &LuaCallExpr,
) -> Option<VarRefId> {
    let prefix_expr = call_expr.get_prefix_expr()?;
    let maybe_func = infer_expr(db, cache, prefix_expr.clone()).ok()?;

    let ret = match maybe_func {
        LuaType::DocFunction(f) => f.get_ret().clone(),
        LuaType::Signature(signature_id) => db
            .get_signature_index()
            .get(&signature_id)?
            .get_return_type(),
        _ => return None,
    };
    let LuaType::Call(alias_call_type) = ret else {
        return None;
    };

    match alias_call_type.get_call_kind() {
        LuaAliasCallKind::RawGet => {
            let args_list = call_expr.get_args_list()?;
            let mut args_iter = args_list.get_args();

            let obj_expr = args_iter.next()?;
            let decl_or_member_id = match get_var_expr_var_ref_id(db, cache, obj_expr.clone()) {
                Some(VarRefId::SelfRef(decl_or_id)) => decl_or_id,
                Some(VarRefId::VarRef(decl_id)) => LuaDeclOrMemberId::Decl(decl_id),
                _ => return None,
            };
            // 开始构建 access_path
            let mut access_path = String::new();
            access_path.push_str(obj_expr.syntax().text().to_string().as_str()); // 这里不需要精确的文本
            access_path.push('.');
            let key_expr = args_iter.next()?;
            match key_expr {
                LuaExpr::LiteralExpr(literal_expr) => match literal_expr.get_literal()? {
                    LuaLiteralToken::String(string_token) => {
                        access_path.push_str(string_token.get_value().as_str());
                    }
                    LuaLiteralToken::Number(number_token) => {
                        access_path.push_str(number_token.get_number_value().to_string().as_str());
                    }
                    _ => return None,
                },
                LuaExpr::NameExpr(name_expr) => {
                    access_path.push_str(name_expr.get_access_path()?.as_str());
                }
                LuaExpr::IndexExpr(index_expr) => {
                    access_path.push_str(index_expr.get_access_path()?.as_str());
                }
                _ => return None,
            }

            Some(VarRefId::IndexRef(
                decl_or_member_id,
                ArcIntern::new(SmolStr::new(access_path)),
            ))
        }
        _ => None,
    }
}

pub fn get_var_expr_var_ref_id(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    var_expr: LuaExpr,
) -> Option<VarRefId> {
    if let Some(var_ref_id) = cache.expr_var_ref_id_cache.get(&var_expr.get_syntax_id()) {
        return Some(var_ref_id.clone());
    }

    let ref_id = match &var_expr {
        LuaExpr::NameExpr(name_expr) => get_name_expr_var_ref_id(db, cache, name_expr),
        LuaExpr::IndexExpr(index_expr) => get_index_expr_var_ref_id(db, cache, index_expr),
        LuaExpr::CallExpr(call_expr) => get_call_expr_var_ref_id(db, cache, call_expr),
        _ => None,
    }?;

    cache
        .expr_var_ref_id_cache
        .insert(var_expr.get_syntax_id(), ref_id.clone());
    Some(ref_id)
}

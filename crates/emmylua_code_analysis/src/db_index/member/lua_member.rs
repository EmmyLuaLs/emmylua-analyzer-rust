use std::ops::Deref;

use emmylua_parser::{LuaIndexKey, LuaSyntaxId, LuaSyntaxKind, NumberResult};
use rowan::{TextRange, TextSize};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use super::lua_member_feature::LuaMemberFeature;
use crate::{DbIndex, FileId, GlobalId, InferFailReason, LuaInferCache, LuaType, infer_expr};

#[derive(Debug)]
pub struct LuaMember {
    member_id: LuaMemberId,
    key: LuaMemberKey,
    feature: LuaMemberFeature,
    global_id: Option<GlobalId>,
}

impl LuaMember {
    pub fn new(
        member_id: LuaMemberId,
        key: LuaMemberKey,
        decl_feature: LuaMemberFeature,
        global_path: Option<GlobalId>,
    ) -> Self {
        Self {
            member_id,
            key,
            feature: decl_feature,
            global_id: global_path,
        }
    }

    pub fn get_key(&self) -> &LuaMemberKey {
        &self.key
    }

    pub fn get_file_id(&self) -> FileId {
        self.member_id.file_id
    }

    pub fn get_range(&self) -> TextRange {
        self.member_id.get_syntax_id().get_range()
    }

    pub fn get_sort_key(&self) -> u64 {
        let file_id = self.member_id.file_id.id;
        let pos = u32::from(self.member_id.id.get_range().start());
        (file_id as u64) << 32 | pos as u64
    }

    pub fn get_syntax_id(&self) -> LuaSyntaxId {
        *self.member_id.get_syntax_id()
    }

    pub fn get_id(&self) -> LuaMemberId {
        self.member_id
    }

    pub fn is_field(&self) -> bool {
        LuaSyntaxKind::DocTagField == self.member_id.get_syntax_id().get_kind()
    }

    pub fn get_feature(&self) -> LuaMemberFeature {
        self.feature
    }

    pub fn get_global_id(&self) -> Option<&GlobalId> {
        self.global_id.as_ref()
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash, Serialize, Deserialize)]
pub struct LuaMemberId {
    pub file_id: FileId,
    id: LuaSyntaxId,
}

impl LuaMemberId {
    pub fn new(id: LuaSyntaxId, file_id: FileId) -> Self {
        Self { id, file_id }
    }

    pub fn get_syntax_id(&self) -> &LuaSyntaxId {
        &self.id
    }

    pub fn get_position(&self) -> TextSize {
        self.id.get_range().start()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LuaMemberKey {
    None,
    Integer(i64),
    Name(SmolStr),
    ExprType(LuaType),
}

impl LuaMemberKey {
    pub fn from_index_key(
        db: &DbIndex,
        cache: &mut LuaInferCache,
        key: &LuaIndexKey,
    ) -> Result<Self, InferFailReason> {
        match key {
            LuaIndexKey::Name(name) => Ok(LuaMemberKey::Name(name.get_name_text().into())),
            LuaIndexKey::String(str) => Ok(LuaMemberKey::Name(str.get_value().into())),
            LuaIndexKey::Integer(i) => {
                if let NumberResult::Int(idx) = i.get_number_value() {
                    Ok(LuaMemberKey::Integer(idx))
                } else {
                    Err(InferFailReason::FieldNotFound)
                }
            }
            LuaIndexKey::Idx(idx) => Ok(LuaMemberKey::Integer(*idx as i64)),
            LuaIndexKey::Expr(expr) => {
                let expr_type = infer_expr(db, cache, expr.clone())?;
                match expr_type {
                    LuaType::StringConst(s) => Ok(LuaMemberKey::Name(s.deref().clone())),
                    LuaType::DocStringConst(s) => Ok(LuaMemberKey::Name(s.deref().clone())),
                    LuaType::IntegerConst(i) => Ok(LuaMemberKey::Integer(i)),
                    LuaType::DocIntegerConst(i) => Ok(LuaMemberKey::Integer(i)),
                    _ => Ok(LuaMemberKey::ExprType(expr_type)),
                }
            }
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, LuaMemberKey::None)
    }

    pub fn is_name(&self) -> bool {
        matches!(self, LuaMemberKey::Name(_))
    }

    pub fn is_integer(&self) -> bool {
        matches!(self, LuaMemberKey::Integer(_))
    }

    pub fn is_expr(&self) -> bool {
        matches!(self, LuaMemberKey::ExprType(_))
    }

    pub fn get_name(&self) -> Option<&str> {
        match self {
            LuaMemberKey::Name(name) => Some(name.as_ref()),
            _ => None,
        }
    }

    pub fn get_integer(&self) -> Option<i64> {
        match self {
            LuaMemberKey::Integer(i) => Some(*i),
            _ => None,
        }
    }

    pub fn to_path(&self) -> String {
        match self {
            LuaMemberKey::Name(name) => name.to_string(),
            LuaMemberKey::Integer(i) => {
                format!("[{}]", i)
            }
            LuaMemberKey::None => "".to_string(),
            LuaMemberKey::ExprType(_) => "".to_string(),
        }
    }
}

impl PartialOrd for LuaMemberKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LuaMemberKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use LuaMemberKey::*;
        match (self, other) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, _) => std::cmp::Ordering::Less,
            (_, None) => std::cmp::Ordering::Greater,
            (Integer(a), Integer(b)) => a.cmp(b),
            (Integer(_), _) => std::cmp::Ordering::Less,
            (_, Integer(_)) => std::cmp::Ordering::Greater,
            (Name(a), Name(b)) => a.cmp(b),
            (Name(_), _) => std::cmp::Ordering::Less,
            (_, Name(_)) => std::cmp::Ordering::Greater,
            (ExprType(_), ExprType(_)) => std::cmp::Ordering::Equal,
        }
    }
}

impl From<String> for LuaMemberKey {
    fn from(name: String) -> Self {
        LuaMemberKey::Name(name.into())
    }
}

impl From<i64> for LuaMemberKey {
    fn from(i: i64) -> Self {
        LuaMemberKey::Integer(i)
    }
}

impl From<&str> for LuaMemberKey {
    fn from(name: &str) -> Self {
        LuaMemberKey::Name(name.to_string().into())
    }
}

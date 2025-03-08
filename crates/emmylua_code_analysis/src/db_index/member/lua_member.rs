use emmylua_parser::{LuaDocFieldKey, LuaIndexKey, LuaSyntaxId, LuaSyntaxKind};
use rowan::TextRange;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use crate::{
    db_index::{LuaType, LuaTypeDeclId},
    FileId, InFiled,
};

#[derive(Debug)]
pub struct LuaMember {
    pub(super) owner: LuaMemberOwner,
    key: LuaMemberKey,
    file_id: FileId,
    syntax_id: LuaSyntaxId,
    decl_type: Option<LuaType>,
}

impl LuaMember {
    pub fn new(
        owner: LuaMemberOwner,
        key: LuaMemberKey,
        file_id: FileId,
        id: LuaSyntaxId,
        decl_type: Option<LuaType>,
    ) -> Self {
        Self {
            owner,
            key,
            file_id,
            syntax_id: id,
            decl_type,
        }
    }

    pub fn get_owner(&self) -> LuaMemberOwner {
        self.owner.clone()
    }

    pub fn get_key(&self) -> &LuaMemberKey {
        &self.key
    }

    pub fn get_file_id(&self) -> FileId {
        self.file_id
    }

    pub fn get_range(&self) -> TextRange {
        self.syntax_id.get_range()
    }

    pub fn get_syntax_id(&self) -> LuaSyntaxId {
        self.syntax_id
    }

    pub fn get_decl_type(&self) -> LuaType {
        self.decl_type.clone().unwrap_or(LuaType::Unknown)
    }

    pub(crate) fn set_decl_type(&mut self, decl_type: LuaType) {
        self.decl_type = Some(decl_type);
    }

    pub fn get_id(&self) -> LuaMemberId {
        LuaMemberId::new(self.syntax_id, self.file_id)
    }

    pub fn with_owner(&self, owner: LuaMemberOwner) -> Self {
        Self {
            owner,
            key: self.key.clone(),
            file_id: self.file_id,
            syntax_id: self.syntax_id,
            decl_type: self.decl_type.clone(),
        }
    }

    pub fn is_field(&self) -> Option<()> {
        if let LuaSyntaxKind::DocTagField = self.syntax_id.get_kind() {
            Some(())
        } else {
            None
        }
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
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum LuaMemberOwner {
    None,
    Type(LuaTypeDeclId),
    Element(InFiled<TextRange>),
}

impl LuaMemberOwner {
    pub fn get_type_id(&self) -> Option<&LuaTypeDeclId> {
        match self {
            LuaMemberOwner::Type(id) => Some(id),
            _ => None,
        }
    }

    pub fn get_element_range(&self) -> Option<&InFiled<TextRange>> {
        match self {
            LuaMemberOwner::Element(range) => Some(range),
            _ => None,
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, LuaMemberOwner::None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LuaMemberKey {
    None,
    Integer(i64),
    Name(SmolStr),
}

impl LuaMemberKey {
    pub fn is_none(&self) -> bool {
        matches!(self, LuaMemberKey::None)
    }

    pub fn is_name(&self) -> bool {
        matches!(self, LuaMemberKey::Name(_))
    }

    pub fn is_integer(&self) -> bool {
        matches!(self, LuaMemberKey::Integer(_))
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
        }
    }
}

impl From<LuaIndexKey> for LuaMemberKey {
    fn from(key: LuaIndexKey) -> Self {
        match key {
            LuaIndexKey::Name(name) => LuaMemberKey::Name(name.get_name_text().into()),
            LuaIndexKey::String(str) => LuaMemberKey::Name(str.get_value().into()),
            LuaIndexKey::Integer(i) => LuaMemberKey::Integer(i.get_int_value()),
            LuaIndexKey::Idx(idx) => LuaMemberKey::Integer(idx as i64),
            _ => LuaMemberKey::None,
        }
    }
}

impl From<&LuaIndexKey> for LuaMemberKey {
    fn from(key: &LuaIndexKey) -> Self {
        match key {
            LuaIndexKey::Name(name) => LuaMemberKey::Name(name.get_name_text().to_string().into()),
            LuaIndexKey::String(str) => LuaMemberKey::Name(str.get_value().into()),
            LuaIndexKey::Integer(i) => LuaMemberKey::Integer(i.get_int_value()),
            _ => LuaMemberKey::None,
        }
    }
}

impl From<LuaDocFieldKey> for LuaMemberKey {
    fn from(key: LuaDocFieldKey) -> Self {
        match key {
            LuaDocFieldKey::Name(name) => {
                LuaMemberKey::Name(name.get_name_text().to_string().into())
            }
            LuaDocFieldKey::String(str) => LuaMemberKey::Name(str.get_value().into()),
            LuaDocFieldKey::Integer(i) => LuaMemberKey::Integer(i.get_int_value()),
            _ => LuaMemberKey::None,
        }
    }
}

impl From<&LuaDocFieldKey> for LuaMemberKey {
    fn from(key: &LuaDocFieldKey) -> Self {
        match key {
            LuaDocFieldKey::Name(name) => {
                LuaMemberKey::Name(name.get_name_text().to_string().into())
            }
            LuaDocFieldKey::String(str) => LuaMemberKey::Name(str.get_value().into()),
            LuaDocFieldKey::Integer(i) => LuaMemberKey::Integer(i.get_int_value()),
            _ => LuaMemberKey::None,
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

//! File-independent member keys (for lookup).
//!
//! `MyMod.Field` -> `Name("Field")`; `t[1]` -> `Integer(1)`;
//! Dynamic keys are retained as `TypeKey(LuaType)`.
//! Separated from identity (file + position in `SemanticId::Member`): keys answer "what to find"; identity answers "where it is declared".

use smol_str::SmolStr;

use crate::LuaType;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LuaMemberKey {
    None,
    Integer(i64),
    Name(SmolStr),
    TypeKey(LuaType),
}

impl LuaMemberKey {
    pub fn name(&self) -> Option<&str> {
        self.get_name()
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
        matches!(self, LuaMemberKey::TypeKey(_))
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

    pub fn to_index_type(&self) -> Option<LuaType> {
        match self {
            LuaMemberKey::Integer(i) => Some(LuaType::IntegerConst(*i)),
            LuaMemberKey::Name(name) => Some(LuaType::StringConst(name.clone().into())),
            LuaMemberKey::TypeKey(typ) => Some(typ.clone()),
            LuaMemberKey::None => None,
        }
    }

    pub fn to_path(&self) -> String {
        match self {
            LuaMemberKey::Name(name) => name.to_string(),
            LuaMemberKey::Integer(i) => format!("[{}]", i),
            LuaMemberKey::None => String::new(),
            LuaMemberKey::TypeKey(_) => String::new(),
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
            (TypeKey(_), TypeKey(_)) => std::cmp::Ordering::Equal,
        }
    }
}

impl From<String> for LuaMemberKey {
    fn from(name: String) -> Self {
        LuaMemberKey::Name(name.into())
    }
}

impl From<&str> for LuaMemberKey {
    fn from(name: &str) -> Self {
        LuaMemberKey::Name(name.to_string().into())
    }
}

impl From<SmolStr> for LuaMemberKey {
    fn from(name: SmolStr) -> Self {
        LuaMemberKey::Name(name)
    }
}

impl From<i64> for LuaMemberKey {
    fn from(i: i64) -> Self {
        LuaMemberKey::Integer(i)
    }
}

use smol_str::SmolStr;

use crate::LuaType;

// 仅包含具名键, 索引签名的键类型由语义模型推断
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LuaMemberKey {
    None,
    Integer(i64),
    Name(SmolStr),
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
            LuaMemberKey::None => None,
        }
    }

    pub fn to_path(&self) -> String {
        match self {
            LuaMemberKey::Name(name) => name.to_string(),
            LuaMemberKey::Integer(i) => format!("[{}]", i),
            LuaMemberKey::None => String::new(),
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

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaVersionNumber {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl LuaVersionNumber {
    #[allow(unused)]
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    #[allow(unused)]
    pub const LUA_JIT: Self = Self {
        major: 2,
        minor: 0,
        patch: 0,
    };

    pub fn from_str(s: &str) -> Option<Self> {
        if s == "JIT" {
            return Some(Self::LUA_JIT);
        }

        let mut iter = s.split('.').map(|it| it.parse::<u32>().unwrap_or(0));
        let major = iter.next().unwrap_or(0);
        let minor = iter.next().unwrap_or(0);
        let patch = iter.next().unwrap_or(0);
        Some(Self {
            major,
            minor,
            patch,
        })
    }
}

impl PartialOrd for LuaVersionNumber {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LuaVersionNumber {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch))
    }
}

impl fmt::Display for LuaVersionNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            LuaVersionNumber::LUA_JIT => write!(f, "Lua JIT"),
            LuaVersionNumber { major, minor, .. } => write!(f, "Lua {}.{}", major, minor),
        }
    }
}

#[allow(unused)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LuaVersionCondition {
    Eq(LuaVersionNumber),
    Gte(LuaVersionNumber),
    Lte(LuaVersionNumber),
}

#[allow(unused)]
impl LuaVersionCondition {
    pub fn check(&self, version: &LuaVersionNumber) -> bool {
        match self {
            LuaVersionCondition::Eq(v) => version == v,
            LuaVersionCondition::Gte(v) => version >= v,
            LuaVersionCondition::Lte(v) => version <= v,
        }
    }
}

impl fmt::Display for LuaVersionCondition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LuaVersionCondition::Eq(v) => write!(f, "{}", v),
            LuaVersionCondition::Gte(v) => write!(f, ">= {}", v),
            LuaVersionCondition::Lte(v) => write!(f, "<= {}", v),
        }
    }
}

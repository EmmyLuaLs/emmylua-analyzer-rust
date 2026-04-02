use crate::{LuaType, TypeVisitTrait};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BasicTypeUnion(u32);

impl BasicTypeUnion {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn add(&mut self, ty: BasicTypeKind) {
        self.0 |= 1 << (ty as u32);
    }

    pub fn contains(&self, ty: BasicTypeKind) -> bool {
        (self.0 & (1 << (ty as u32))) != 0
    }

    pub fn remove(&mut self, ty: BasicTypeKind) {
        self.0 &= !(1 << (ty as u32));
    }

    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = LuaType> + '_ {
        (0..BasicTypeKind::Count as u32)
            .filter(move |&i| (self.0 & (1 << i)) != 0)
            .map(|i| BasicTypeKind::from(i).into())
    }
}

impl TypeVisitTrait for BasicTypeUnion {
    fn visit_type<F>(&self, f: &mut F)
    where
        F: FnMut(&LuaType),
    {
        for ty in self.iter() {
            f(&ty);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BasicTypeKind {
    Unknown,
    Any,
    Nil,
    Table,
    Userdata,
    Function,
    Thread,
    Boolean,
    String,
    Integer,
    Number,
    Io,
    SelfInfer,
    Global,
    Never,
    Count,
}

impl From<u32> for BasicTypeKind {
    fn from(value: u32) -> BasicTypeKind {
        match value {
            0 => BasicTypeKind::Unknown,
            1 => BasicTypeKind::Any,
            2 => BasicTypeKind::Nil,
            3 => BasicTypeKind::Table,
            4 => BasicTypeKind::Userdata,
            5 => BasicTypeKind::Function,
            6 => BasicTypeKind::Thread,
            7 => BasicTypeKind::Boolean,
            8 => BasicTypeKind::String,
            9 => BasicTypeKind::Integer,
            10 => BasicTypeKind::Number,
            11 => BasicTypeKind::Io,
            12 => BasicTypeKind::SelfInfer,
            13 => BasicTypeKind::Global,
            14 => BasicTypeKind::Never,
            _ => unreachable!(),
        }
    }
}

impl From<BasicTypeKind> for LuaType {
    fn from(value: BasicTypeKind) -> LuaType {
        match value {
            BasicTypeKind::Unknown => LuaType::Unknown,
            BasicTypeKind::Any => LuaType::Any,
            BasicTypeKind::Nil => LuaType::Nil,
            BasicTypeKind::Table => LuaType::Table,
            BasicTypeKind::Userdata => LuaType::Userdata,
            BasicTypeKind::Function => LuaType::Function,
            BasicTypeKind::Thread => LuaType::Thread,
            BasicTypeKind::Boolean => LuaType::Boolean,
            BasicTypeKind::String => LuaType::String,
            BasicTypeKind::Integer => LuaType::Integer,
            BasicTypeKind::Number => LuaType::Number,
            BasicTypeKind::Io => LuaType::Io,
            BasicTypeKind::SelfInfer => LuaType::SelfInfer,
            BasicTypeKind::Global => LuaType::Global,
            BasicTypeKind::Never => LuaType::Never,
            BasicTypeKind::Count => unreachable!(),
        }
    }
}

impl BasicTypeKind {
    pub fn from_type(value: &LuaType) -> Option<BasicTypeKind> {
        match value {
            LuaType::Unknown => Some(BasicTypeKind::Unknown),
            LuaType::Any => Some(BasicTypeKind::Any),
            LuaType::Nil => Some(BasicTypeKind::Nil),
            LuaType::Table => Some(BasicTypeKind::Table),
            LuaType::Userdata => Some(BasicTypeKind::Userdata),
            LuaType::Function => Some(BasicTypeKind::Function),
            LuaType::Thread => Some(BasicTypeKind::Thread),
            LuaType::Boolean => Some(BasicTypeKind::Boolean),
            LuaType::String => Some(BasicTypeKind::String),
            LuaType::Integer => Some(BasicTypeKind::Integer),
            LuaType::Number => Some(BasicTypeKind::Number),
            LuaType::Io => Some(BasicTypeKind::Io),
            LuaType::SelfInfer => Some(BasicTypeKind::SelfInfer),
            LuaType::Global => Some(BasicTypeKind::Global),
            LuaType::Never => Some(BasicTypeKind::Never),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{BasicTypeKind, BasicTypeUnion, LuaType};

    #[test]
    fn test_basic_type_union() {
        let mut union = BasicTypeUnion::new();
        assert!(union.is_empty());

        union.add(BasicTypeKind::Nil);
        assert!(union.contains(BasicTypeKind::Nil));
        assert!(!union.contains(BasicTypeKind::Table));

        union.add(BasicTypeKind::Table);
        assert!(union.contains(BasicTypeKind::Nil));
        assert!(union.contains(BasicTypeKind::Table));

        union.remove(BasicTypeKind::Nil);
        assert!(!union.contains(BasicTypeKind::Nil));
        assert!(union.contains(BasicTypeKind::Table));

        let types: Vec<LuaType> = union.iter().collect();
        assert_eq!(types, vec![LuaType::Table]);
    }
}

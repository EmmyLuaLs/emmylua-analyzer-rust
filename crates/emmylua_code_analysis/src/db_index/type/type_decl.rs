use flagset::{FlagSet, flags};
use internment::ArcIntern;
use rowan::TextRange;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::SmolStr;

use crate::{
    DbIndex, FileId, LuaMemberKey, LuaMemberOwner, TypeSubstitutor, instantiate_type_generic,
};

use super::{LuaType, LuaUnionType};

#[derive(Debug, Eq, PartialEq, Hash, Clone, Copy)]
pub enum LuaDeclTypeKind {
    Class,
    Enum,
    Alias,
    Attribute,
}

flags! {
    pub enum LuaTypeFlag: u8 {
        None,
        Key,
        Partial,
        Exact,
        Meta,
        Constructor,
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct LuaTypeDecl {
    simple_name: String,
    locations: Vec<LuaDeclLocation>,
    id: LuaTypeDeclId,
    extra: LuaTypeExtra,
}

impl LuaTypeDecl {
    pub fn new(
        file_id: FileId,
        range: TextRange,
        name: String,
        kind: LuaDeclTypeKind,
        flag: FlagSet<LuaTypeFlag>,
        id: LuaTypeDeclId,
    ) -> Self {
        Self {
            simple_name: name,
            locations: vec![LuaDeclLocation {
                file_id,
                range,
                flag,
            }],
            id,
            extra: match kind {
                LuaDeclTypeKind::Enum => LuaTypeExtra::Enum { base: None },
                LuaDeclTypeKind::Class => LuaTypeExtra::Class,
                LuaDeclTypeKind::Alias => LuaTypeExtra::Alias { origin: None },
                LuaDeclTypeKind::Attribute => LuaTypeExtra::Attribute { typ: None },
            },
        }
    }

    pub fn get_locations(&self) -> &[LuaDeclLocation] {
        &self.locations
    }

    pub fn get_mut_locations(&mut self) -> &mut Vec<LuaDeclLocation> {
        &mut self.locations
    }

    pub fn get_name(&self) -> &str {
        &self.simple_name
    }

    pub fn is_class(&self) -> bool {
        matches!(self.extra, LuaTypeExtra::Class)
    }

    pub fn is_enum(&self) -> bool {
        matches!(self.extra, LuaTypeExtra::Enum { .. })
    }

    pub fn is_alias(&self) -> bool {
        matches!(self.extra, LuaTypeExtra::Alias { .. })
    }

    pub fn is_attribute(&self) -> bool {
        matches!(self.extra, LuaTypeExtra::Attribute { .. })
    }

    pub fn is_exact(&self) -> bool {
        self.locations
            .iter()
            .any(|l| l.flag.contains(LuaTypeFlag::Exact))
    }

    pub fn is_partial(&self) -> bool {
        self.locations
            .iter()
            .any(|l| l.flag.contains(LuaTypeFlag::Partial))
    }

    pub fn is_enum_key(&self) -> bool {
        self.locations
            .iter()
            .any(|l| l.flag.contains(LuaTypeFlag::Key))
    }

    pub fn get_id(&self) -> LuaTypeDeclId {
        self.id.clone()
    }

    pub fn get_full_name(&self) -> &str {
        self.id.get_name()
    }

    pub fn get_namespace(&self) -> Option<&str> {
        self.id
            .get_name()
            .rfind('.')
            .map(|idx| &self.id.get_name()[..idx])
    }

    pub fn get_alias_origin(
        &self,
        db: &DbIndex,
        substitutor: Option<&TypeSubstitutor>,
    ) -> Option<LuaType> {
        match &self.extra {
            LuaTypeExtra::Alias {
                origin: Some(origin),
            } => {
                let substitutor = match substitutor {
                    Some(substitutor) => substitutor,
                    None => return Some(origin.clone()),
                };

                let type_decl_id = self.get_id();
                if db
                    .get_type_index()
                    .get_generic_params(&type_decl_id)
                    .is_none()
                {
                    return Some(origin.clone());
                }

                Some(instantiate_type_generic(db, origin, substitutor))
            }
            _ => None,
        }
    }

    pub fn get_alias_ref(&self) -> Option<&LuaType> {
        match &self.extra {
            LuaTypeExtra::Alias { origin, .. } => origin.as_ref(),
            _ => None,
        }
    }

    pub fn add_alias_origin(&mut self, replace: LuaType) {
        if let LuaTypeExtra::Alias { origin, .. } = &mut self.extra {
            *origin = Some(replace);
        }
    }

    pub fn add_enum_base(&mut self, base_type: LuaType) {
        if let LuaTypeExtra::Enum { base } = &mut self.extra {
            *base = Some(base_type);
        }
    }

    pub fn add_attribute_type(&mut self, attribute_type: LuaType) {
        if let LuaTypeExtra::Attribute { typ } = &mut self.extra {
            *typ = Some(attribute_type);
        }
    }

    pub fn get_attribute_type(&self) -> Option<&LuaType> {
        if let LuaTypeExtra::Attribute { typ: Some(typ) } = &self.extra {
            Some(typ)
        } else {
            None
        }
    }

    pub fn merge_decl(&mut self, other: LuaTypeDecl) {
        self.locations.extend(other.locations);
    }

    /// 获取枚举字段的类型
    pub fn get_enum_field_type(&self, db: &DbIndex) -> Option<LuaType> {
        if !self.is_enum() {
            return None;
        }

        let enum_member_owner = LuaMemberOwner::Type(self.get_id());
        let enum_members = db.get_member_index().get_members(&enum_member_owner)?;

        let mut union_types = Vec::new();
        if self.is_enum_key() {
            for enum_member in enum_members {
                let member_key = enum_member.get_key();
                let fake_type = match member_key {
                    LuaMemberKey::Name(name) => LuaType::DocStringConst(name.clone().into()),
                    LuaMemberKey::Integer(i) => LuaType::IntegerConst(*i),
                    LuaMemberKey::ExprType(typ) => typ.clone(),
                    LuaMemberKey::None => continue,
                };

                union_types.push(fake_type);
            }
        } else {
            for member in enum_members {
                if let Some(type_cache) =
                    db.get_type_index().get_type_cache(&member.get_id().into())
                {
                    let member_fake_type = match type_cache.as_type() {
                        LuaType::StringConst(s) => LuaType::DocStringConst(s.clone()),
                        LuaType::IntegerConst(i) => LuaType::DocIntegerConst(*i),
                        _ => type_cache.as_type().clone(),
                    };

                    union_types.push(member_fake_type);
                }
            }
        }

        Some(LuaType::Union(LuaUnionType::from_vec(union_types).into()))
    }
}

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub struct LuaTypeDeclId {
    id: ArcIntern<SmolStr>,
}

impl LuaTypeDeclId {
    pub fn new_by_id(id: ArcIntern<SmolStr>) -> Self {
        Self { id }
    }

    pub fn new(str: &str) -> Self {
        Self {
            id: ArcIntern::new(SmolStr::new(str)),
        }
    }

    pub fn get_name(&self) -> &str {
        &self.id
    }

    pub fn get_simple_name(&self) -> &str {
        let basic_name = self.get_name();

        (if let Some(i) = basic_name.rfind('.') {
            &basic_name[i + 1..]
        } else {
            basic_name
        }) as _
    }

    pub fn collect_super_types(&self, db: &DbIndex, collected_types: &mut Vec<LuaType>) {
        // 必须广度优先
        let mut queue = Vec::new();
        queue.push(self.clone());

        while let Some(current_id) = queue.pop() {
            let super_types = db.get_type_index().get_super_types(&current_id);
            if let Some(super_types) = super_types {
                for super_type in super_types {
                    match &super_type {
                        LuaType::Ref(super_type_id) => {
                            if !collected_types.contains(&super_type) {
                                collected_types.push(super_type.clone());
                                queue.push(super_type_id.clone());
                            }
                        }
                        _ => {
                            if !collected_types.contains(&super_type) {
                                collected_types.push(super_type.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn collect_super_types_with_self(&self, db: &DbIndex, typ: LuaType) -> Vec<LuaType> {
        let mut collected_types: Vec<LuaType> = vec![typ];
        self.collect_super_types(db, &mut collected_types);
        collected_types
    }
}

impl Serialize for LuaTypeDeclId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.id)
    }
}

impl<'de> Deserialize<'de> for LuaTypeDeclId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(LuaTypeDeclId {
            id: ArcIntern::new(SmolStr::new(s)),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaDeclLocation {
    pub file_id: FileId,
    pub range: TextRange,
    pub flag: FlagSet<LuaTypeFlag>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum LuaTypeExtra {
    Enum { base: Option<LuaType> },
    Class,
    Alias { origin: Option<LuaType> },
    Attribute { typ: Option<LuaType> },
}

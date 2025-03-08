use flagset::{flags, FlagSet};
use internment::ArcIntern;
use rowan::TextRange;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::SmolStr;

use crate::{instantiate_type, DbIndex, FileId, TypeSubstitutor};

use super::LuaType;

#[derive(Debug, Eq, PartialEq, Hash, Clone, Copy)]
pub enum LuaDeclTypeKind {
    Class,
    Enum,
    Alias,
}

flags! {
    pub enum LuaTypeAttribute: u8 {
        None,
        Key,
        Local,
        // Global,
        Partial,
        Env,
        Exact
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct LuaTypeDecl {
    simple_name: String,
    pub(crate) attrib: Option<FlagSet<LuaTypeAttribute>>,
    pub(crate) locations: Vec<LuaDeclLocation>,
    id: LuaTypeDeclId,
    extra: Box<LuaTypeExtra>,
}

impl LuaTypeDecl {
    pub fn new(
        file_id: FileId,
        range: TextRange,
        name: String,
        kind: LuaDeclTypeKind,
        attrib: Option<FlagSet<LuaTypeAttribute>>,
        id: LuaTypeDeclId,
    ) -> Self {
        Self {
            simple_name: name,
            attrib,
            locations: vec![LuaDeclLocation { file_id, range }],
            id,
            extra: match kind {
                LuaDeclTypeKind::Enum => Box::new(LuaTypeExtra::Enum { base: None }),
                LuaDeclTypeKind::Class => Box::new(LuaTypeExtra::Class),
                LuaDeclTypeKind::Alias => Box::new(LuaTypeExtra::Alias { origin: None }),
            },
        }
    }

    #[allow(unused)]
    pub fn get_file_ids(&self) -> Vec<FileId> {
        self.locations.iter().map(|loc| loc.file_id).collect()
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

    pub fn get_kind(&self) -> LuaDeclTypeKind {
        match &*self.extra {
            LuaTypeExtra::Enum { .. } => LuaDeclTypeKind::Enum,
            LuaTypeExtra::Class => LuaDeclTypeKind::Class,
            LuaTypeExtra::Alias { .. } => LuaDeclTypeKind::Alias,
        }
    }

    pub fn is_class(&self) -> bool {
        matches!(&*self.extra, LuaTypeExtra::Class)
    }

    pub fn is_enum(&self) -> bool {
        matches!(&*self.extra, LuaTypeExtra::Enum { .. })
    }

    pub fn is_alias(&self) -> bool {
        matches!(&*self.extra, LuaTypeExtra::Alias { .. })
    }

    pub fn is_env(&self) -> bool {
        if let Some(attrlib) = self.get_attrib() {
            return attrlib.contains(LuaTypeAttribute::Env);
        }
        
        false
    }

    pub fn get_attrib(&self) -> Option<FlagSet<LuaTypeAttribute>> {
        self.attrib
    }

    pub fn is_exact(&self) -> bool {
        self.attrib
            .map_or(false, |a| a.contains(LuaTypeAttribute::Exact))
    }

    pub fn is_partial(&self) -> bool {
        self.attrib
            .map_or(false, |a| a.contains(LuaTypeAttribute::Partial))
    }

    pub fn is_enum_key(&self) -> bool {
        self.attrib
            .map_or(false, |a| a.contains(LuaTypeAttribute::Key))
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
        match &*self.extra {
            LuaTypeExtra::Alias {
                origin: Some(origin),
            } => {
                if substitutor.is_none() {
                    return Some(origin.clone());
                }

                let type_decl_id = self.get_id();
                if db
                    .get_type_index()
                    .get_generic_params(&type_decl_id)
                    .is_none()
                {
                    return Some(origin.clone());
                }

                let substitutor = substitutor.unwrap();
                Some(instantiate_type(db, origin, substitutor))
            }
            _ => None,
        }
    }

    pub fn add_alias_origin(&mut self, replace: LuaType) {
        match &mut *self.extra {
            LuaTypeExtra::Alias { origin, .. } => {
                *origin = Some(replace);
            }
            _ => {}
        }
    }

    pub fn add_enum_base(&mut self, base_type: LuaType) {
        match &mut *self.extra {
            LuaTypeExtra::Enum { base } => {
                *base = Some(base_type);
            }
            _ => {}
        }
    }
}

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub struct LuaTypeDeclId {
    id: ArcIntern<SmolStr>,
}

impl LuaTypeDeclId {
    #[allow(unused)]
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
        let just_name = if let Some(i) = basic_name.rfind('.') {
            &basic_name[i + 1..]
        } else {
            &basic_name
        };

        &just_name
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
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum LuaTypeExtra {
    Enum { base: Option<LuaType> },
    Class,
    Alias { origin: Option<LuaType> },
}

use std::fmt;

use flagset::{FlagSet, flags};
use internment::ArcIntern;
use rowan::TextRange;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::SmolStr;

use crate::{FileId, WorkspaceId};

use super::LuaType;

#[derive(Debug, Eq, PartialEq, Hash, Clone, Copy)]
pub enum LuaDeclTypeKind {
    Class,
    Enum,
    Alias,
    Attribute,
}

flags! {
    pub enum LuaTypeFlag: u8 {
        Key,
        Partial,
        Exact,
        Meta,
        Constructor,
        Public,
        Internal,
        Private
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
        let location = LuaDeclLocation {
            file_id,
            range,
            flag,
        };
        Self {
            simple_name: name,
            locations: vec![location],
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
}

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub enum LuaTypeIdentifier {
    Global(SmolStr),
    Internal(WorkspaceId, SmolStr),
    Local(FileId, SmolStr),
}

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub struct LuaTypeDeclId {
    id: ArcIntern<LuaTypeIdentifier>,
}

impl LuaTypeDeclId {
    pub fn global(str: &str) -> Self {
        Self {
            id: ArcIntern::new(LuaTypeIdentifier::Global(SmolStr::new(str))),
        }
    }

    pub fn local(file_id: FileId, str: &str) -> Self {
        Self {
            id: ArcIntern::new(LuaTypeIdentifier::Local(file_id, SmolStr::new(str))),
        }
    }

    pub fn internal(workspace_id: WorkspaceId, str: &str) -> Self {
        Self {
            id: ArcIntern::new(LuaTypeIdentifier::Internal(workspace_id, SmolStr::new(str))),
        }
    }

    pub fn get_id(&self) -> &LuaTypeIdentifier {
        self.id.as_ref()
    }

    pub fn get_name(&self) -> &str {
        match self.id.as_ref() {
            LuaTypeIdentifier::Global(name) => name.as_ref(),
            LuaTypeIdentifier::Internal(_, name) => name.as_ref(),
            LuaTypeIdentifier::Local(_, name) => name.as_ref(),
        }
    }

    pub fn get_simple_name(&self) -> &str {
        let basic_name = self.get_name();

        (if let Some(i) = basic_name.rfind('.') {
            &basic_name[i + 1..]
        } else {
            basic_name
        }) as _
    }

    pub fn is_local(&self) -> bool {
        matches!(self.id.as_ref(), LuaTypeIdentifier::Local(_, _))
    }
}

impl Serialize for LuaTypeDeclId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.id.as_ref() {
            LuaTypeIdentifier::Global(name) => serializer.serialize_str(name.as_ref()),
            LuaTypeIdentifier::Internal(workspace_id, name) => {
                let s = format!("ws:{}|{}", workspace_id.id, &name);
                serializer.serialize_str(&s)
            }
            LuaTypeIdentifier::Local(file_id, name) => {
                let s = format!("{}|{}", file_id.id, &name);
                serializer.serialize_str(&s)
            }
        }
    }
}

impl<'de> Deserialize<'de> for LuaTypeDeclId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LuaTypeDeclIdVisitor;

        impl<'de> serde::de::Visitor<'de> for LuaTypeDeclIdVisitor {
            type Value = LuaTypeDeclId;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string representing LuaTypeDeclId")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if let Some((file_id_str, name)) = value.split_once('|') {
                    if let Some(workspace_id_str) = file_id_str.strip_prefix("ws:") {
                        let workspace_id = workspace_id_str.parse::<u32>().map_err(E::custom)?;
                        return Ok(LuaTypeDeclId::internal(
                            WorkspaceId { id: workspace_id },
                            name,
                        ));
                    }
                    let file_id = file_id_str.parse::<u32>().map_err(E::custom)?;
                    Ok(LuaTypeDeclId::local(FileId { id: file_id }, name))
                } else {
                    Ok(LuaTypeDeclId::global(value))
                }
            }
        }

        deserializer.deserialize_str(LuaTypeDeclIdVisitor)
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

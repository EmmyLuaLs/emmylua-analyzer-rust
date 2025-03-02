use std::fmt;

use crate::LuaSignatureId;
use crate::{db_index::LuaType, FileId};
use emmylua_parser::{LuaKind, LuaSyntaxId, LuaSyntaxKind};
use rowan::{TextRange, TextSize};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::SmolStr;

#[derive(Eq, PartialEq, Hash, Debug, Clone)]
pub struct LuaDecl {
    name: SmolStr,
    file_id: FileId,
    range: TextRange,
    expr_id: Option<LuaSyntaxId>,
    pub extra: LuaDeclExtra,
}

#[derive(Eq, PartialEq, Hash, Debug, Clone)]
pub enum LuaDeclExtra {
    Local {
        kind: LuaKind,
        decl_type: Option<LuaType>,
        attrib: Option<LocalAttribute>,
    },
    Param {
        idx: usize,
        signature_id: LuaSignatureId,
    },
    Global {
        kind: LuaKind,
        decl_type: Option<LuaType>,
    },
}

impl LuaDecl {
    pub fn new(
        name: &str,
        file_id: FileId,
        range: TextRange,
        extra: LuaDeclExtra,
        expr_id: Option<LuaSyntaxId>,
    ) -> Self {
        Self {
            name: SmolStr::new(name),
            file_id,
            range,
            expr_id,
            extra,
        }
    }

    pub fn get_file_id(&self) -> FileId {
        self.file_id
    }

    pub fn get_id(&self) -> LuaDeclId {
        LuaDeclId::new(self.file_id, self.range.start())
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_position(&self) -> TextSize {
        self.range.start()
    }

    pub fn get_range(&self) -> TextRange {
        self.range
    }

    pub fn get_type(&self) -> Option<&LuaType> {
        match &self.extra {
            LuaDeclExtra::Local { decl_type, .. } => decl_type.as_ref(),
            LuaDeclExtra::Global { decl_type, .. } => decl_type.as_ref(),
            LuaDeclExtra::Param { .. } => None,
        }
    }

    pub(crate) fn set_decl_type(&mut self, decl_type: LuaType) {
        match &mut self.extra {
            LuaDeclExtra::Local { decl_type: dt, .. } => *dt = Some(decl_type),
            LuaDeclExtra::Global { decl_type: dt, .. } => *dt = Some(decl_type),
            LuaDeclExtra::Param { .. } => {}
        }
    }

    pub fn get_syntax_id(&self) -> LuaSyntaxId {
        match self.extra {
            LuaDeclExtra::Local { kind, .. } => LuaSyntaxId::new(kind, self.range),
            LuaDeclExtra::Param { .. } => {
                LuaSyntaxId::new(LuaSyntaxKind::ParamName.into(), self.range)
            }
            LuaDeclExtra::Global { kind, .. } => LuaSyntaxId::new(kind, self.range),
        }
    }

    pub fn get_value_syntax_id(&self) -> Option<LuaSyntaxId> {
        self.expr_id
    }

    pub fn is_local(&self) -> bool {
        matches!(
            &self.extra,
            LuaDeclExtra::Local { .. } | LuaDeclExtra::Param { .. }
        )
    }

    pub fn is_param(&self) -> bool {
        matches!(&self.extra, LuaDeclExtra::Param { .. })
    }

    pub fn is_global(&self) -> bool {
        matches!(&self.extra, LuaDeclExtra::Global { .. })
    }

    pub fn set_extra_local(&mut self) {
        match &self.extra {
            LuaDeclExtra::Global { kind, decl_type } => {
                self.extra = LuaDeclExtra::Local {
                    kind: *kind,
                    decl_type: decl_type.clone(),
                    attrib: None,
                };
            }
            _ => {}
        }
    }
}

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub struct LuaDeclId {
    pub file_id: FileId,
    pub position: TextSize,
}

impl Serialize for LuaDeclId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value = format!("{}|{}", self.file_id.id, u32::from(self.position));
        serializer.serialize_str(&value)
    }
}

impl<'de> Deserialize<'de> for LuaDeclId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LuaDeclIdVisitor;

        impl<'de> Visitor<'de> for LuaDeclIdVisitor {
            type Value = LuaDeclId;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string with format 'file_id:position'")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let parts: Vec<&str> = value.split('|').collect();
                if parts.len() != 2 {
                    return Err(E::custom("expected format 'file_id:position'"));
                }

                let file_id = FileId {
                    id: parts[0]
                        .parse()
                        .map_err(|e| E::custom(format!("invalid file_id: {}", e)))?,
                };
                let position = TextSize::new(
                    parts[1]
                        .parse()
                        .map_err(|e| E::custom(format!("invalid position: {}", e)))?,
                );

                Ok(LuaDeclId { file_id, position })
            }
        }

        deserializer.deserialize_str(LuaDeclIdVisitor)
    }
}

impl LuaDeclId {
    pub fn new(file_id: FileId, position: TextSize) -> Self {
        Self { file_id, position }
    }
}

#[derive(Eq, PartialEq, Hash, Debug, Clone)]
pub enum LocalAttribute {
    Const,
    Close,
    IterConst,
}

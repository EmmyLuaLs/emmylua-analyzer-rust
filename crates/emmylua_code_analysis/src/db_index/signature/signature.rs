use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::{collections::HashMap, sync::Arc};

use emmylua_parser::{LuaAstNode, LuaClosureExpr, LuaDocFuncType};
use rowan::TextSize;

use crate::{
    db_index::{LuaFunctionType, LuaType},
    FileId,
};
use crate::{SemanticModel, VariadicType};

#[derive(Debug)]
pub struct LuaSignature {
    pub generic_params: Vec<(String, Option<LuaType>)>,
    pub overloads: Vec<Arc<LuaFunctionType>>,
    pub param_docs: HashMap<usize, LuaDocParamInfo>,
    pub params: Vec<String>,
    pub return_docs: Vec<LuaDocReturnInfo>,
    pub resolve_return: SignatureReturnStatus,
    pub is_colon_define: bool,
    pub is_async: bool,
    pub nodiscard: Option<LuaNoDiscard>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LuaNoDiscard {
    NoDiscard,
    NoDiscardWithMessage(Box<String>),
}

impl LuaSignature {
    pub fn new() -> Self {
        Self {
            generic_params: Vec::new(),
            overloads: Vec::new(),
            param_docs: HashMap::new(),
            params: Vec::new(),
            return_docs: Vec::new(),
            resolve_return: SignatureReturnStatus::UnResolve,
            is_colon_define: false,
            is_async: false,
            nodiscard: None,
        }
    }

    pub fn is_generic(&self) -> bool {
        !self.generic_params.is_empty()
    }

    pub fn is_resolve_return(&self) -> bool {
        self.resolve_return != SignatureReturnStatus::UnResolve
    }

    pub fn get_type_params(&self) -> Vec<(String, Option<LuaType>)> {
        let mut type_params = Vec::new();
        for (idx, param_name) in self.params.iter().enumerate() {
            if let Some(param_info) = self.param_docs.get(&idx) {
                type_params.push((param_name.clone(), Some(param_info.type_ref.clone())));
            } else {
                type_params.push((param_name.clone(), None));
            }
        }

        type_params
    }

    pub fn find_param_idx(&self, param_name: &str) -> Option<usize> {
        self.params.iter().position(|name| name == param_name)
    }

    pub fn get_param_info_by_name(&self, param_name: &str) -> Option<&LuaDocParamInfo> {
        // fast enough
        let idx = self.params.iter().position(|name| name == param_name)?;
        self.param_docs.get(&idx)
    }

    pub fn get_param_name_by_id(&self, idx: usize) -> Option<String> {
        if idx < self.params.len() {
            return Some(self.params[idx].clone());
        } else if let Some(name) = self.params.last() {
            if name == "..." {
                return Some(name.clone());
            }
        }

        None
    }

    pub fn get_param_info_by_id(&self, idx: usize) -> Option<&LuaDocParamInfo> {
        if idx < self.params.len() {
            return self.param_docs.get(&idx);
        } else if let Some(name) = self.params.last() {
            if name == "..." {
                return self.param_docs.get(&(self.params.len() - 1));
            }
        }

        None
    }

    pub fn get_return_type(&self) -> LuaType {
        match self.return_docs.len() {
            0 => LuaType::Nil,
            1 => self.return_docs[0].type_ref.clone(),
            _ => LuaType::Variadic(
                VariadicType::Multi(
                    self.return_docs
                        .iter()
                        .map(|info| info.type_ref.clone())
                        .collect(),
                )
                .into(),
            ),
        }
    }

    pub fn is_method(&self, semantic_model: &SemanticModel, owner_type: Option<&LuaType>) -> bool {
        if self.is_colon_define {
            return true;
        }

        if let Some(param_info) = self.get_param_info_by_id(0) {
            if param_info.type_ref.is_self_infer() {
                return true;
            }
            match owner_type {
                Some(owner_type) => semantic_model
                    .type_check(owner_type, &param_info.type_ref)
                    .is_ok(),
                None => param_info.name == "self",
            }
        } else {
            false
        }
    }

    pub fn to_doc_func_type(&self) -> Arc<LuaFunctionType> {
        let params = self.get_type_params();
        let return_type = self.get_return_type();
        let func_type =
            LuaFunctionType::new(self.is_async, self.is_colon_define, params, return_type);
        Arc::new(func_type)
    }

    pub fn to_call_operator_func_type(&self) -> Arc<LuaFunctionType> {
        let mut params = self.get_type_params();
        if params.len() > 0 && !self.is_colon_define {
            params.remove(0);
        }

        let return_type = self.get_return_type();
        let func_type = LuaFunctionType::new(self.is_async, false, params, return_type);
        Arc::new(func_type)
    }
}

#[derive(Debug)]
pub struct LuaDocParamInfo {
    pub name: String,
    pub type_ref: LuaType,
    pub nullable: bool,
    pub description: Option<String>,
}

#[derive(Debug)]
pub struct LuaDocReturnInfo {
    pub name: Option<String>,
    pub type_ref: LuaType,
    pub description: Option<String>,
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy)]
pub struct LuaSignatureId {
    file_id: FileId,
    position: TextSize,
}

impl Serialize for LuaSignatureId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value = format!("{}|{}", self.file_id.id, u32::from(self.position));
        serializer.serialize_str(&value)
    }
}

impl<'de> Deserialize<'de> for LuaSignatureId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LuaSignatureIdVisitor;

        impl<'de> Visitor<'de> for LuaSignatureIdVisitor {
            type Value = LuaSignatureId;

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

                Ok(LuaSignatureId { file_id, position })
            }
        }

        deserializer.deserialize_str(LuaSignatureIdVisitor)
    }
}

impl LuaSignatureId {
    pub fn from_closure(file_id: FileId, closure: &LuaClosureExpr) -> Self {
        Self {
            file_id,
            position: closure.get_position(),
        }
    }

    #[allow(dead_code)]
    pub fn from_doc_func(file_id: FileId, func_type: &LuaDocFuncType) -> Self {
        Self {
            file_id,
            position: func_type.get_position(),
        }
    }

    pub fn get_file_id(&self) -> FileId {
        self.file_id
    }

    pub fn get_position(&self) -> TextSize {
        self.position
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignatureReturnStatus {
    UnResolve,
    DocResolve,
    InferResolve,
}

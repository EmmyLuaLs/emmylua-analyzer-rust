use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Doc {
    pub name: String,
    pub display: Option<String>,
    pub supers: Option<String>,
    pub namespace: Option<String>,
    pub fields: Option<Vec<MemberDoc>>,
    pub methods: Option<Vec<MemberDoc>>,
    pub property: Property,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct MemberDoc {
    pub name: String,
    pub display: String,
    pub property: Property,
    /// Documented parameters of a function member, with their descriptions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<MemberParam>,
    /// Documented return values of a function member, with their descriptions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub returns: Vec<MemberParam>,
}

/// A documented parameter or return value of a function member.
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct MemberParam {
    pub name: String,
    /// Rendered type, when it is known.
    pub type_text: Option<String>,
    /// `@param` / `@return` description, when it is present.
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Property {
    pub description: Option<String>,
    pub see: Option<String>,
    pub deprecated: Option<String>,
    pub other: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct MkdocsIndex {
    pub site_name: String,
    pub types: Vec<IndexStruct>,
    pub modules: Vec<IndexStruct>,
    pub globals: Vec<IndexStruct>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexStruct {
    pub name: String,
    pub file: String,
}

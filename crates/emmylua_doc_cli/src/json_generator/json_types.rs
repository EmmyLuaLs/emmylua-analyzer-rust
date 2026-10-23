use emmylua_code_analysis::Emmyrc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Index {
    pub modules: Vec<Module>,
    pub types: Vec<Type>,
    pub globals: Vec<Global>,
    pub config: Emmyrc,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Global {
    Table(GlobalTable),
    Field(GlobalField),
    Fn(Fn),
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct GlobalTable {
    pub name: String,
    #[serde(flatten)]
    pub property: Property,
    pub loc: Option<Loc>,
    pub members: Vec<Member>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct GlobalField {
    pub name: String,
    #[serde(flatten)]
    pub property: Property,
    pub loc: Option<Loc>,
    pub typ: String,
    pub literal: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Module {
    pub name: String,
    #[serde(flatten)]
    pub property: Property,
    pub file: Option<PathBuf>,
    pub typ: Option<String>,
    pub members: Vec<Member>,
    pub namespace: Option<String>,
    pub using: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Type {
    Class(Class),
    Enum(Enum),
    Alias(Alias),
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Class {
    pub name: String,
    #[serde(flatten)]
    pub property: Property,
    pub loc: Vec<Loc>,
    pub bases: Vec<String>,
    pub generics: Vec<TypeVar>,
    pub members: Vec<Member>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Enum {
    pub name: String,
    #[serde(flatten)]
    pub property: Property,
    pub loc: Vec<Loc>,
    pub typ: Option<String>,
    pub generics: Vec<TypeVar>,
    pub members: Vec<Member>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Alias {
    pub name: String,
    #[serde(flatten)]
    pub property: Property,
    pub loc: Vec<Loc>,
    pub typ: Option<String>,
    pub generics: Vec<TypeVar>,
    pub members: Vec<Member>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Member {
    Fn(Fn),
    Field(Field),
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Fn {
    pub name: String,
    #[serde(flatten)]
    pub property: Property,
    pub loc: Option<Loc>,
    pub generics: Vec<TypeVar>,
    pub params: Vec<FnParam>,
    pub returns: Vec<FnParam>,
    pub overloads: Vec<String>,
    pub is_async: bool,
    pub is_meth: bool,
    pub is_nodiscard: bool,
    pub nodiscard_message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct FnParam {
    pub name: Option<String>,
    pub typ: Option<String>,
    pub desc: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Field {
    pub name: String,
    #[serde(flatten)]
    pub property: Property,
    pub loc: Option<Loc>,
    pub typ: String,
    pub literal: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Property {
    pub description: Option<String>,
    pub visibility: Option<String>,
    pub deprecated: bool,
    pub deprecation_reason: Option<String>,
    pub tag_content: Option<Vec<TagNameContent>>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct TagNameContent {
    pub tag_name: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct TypeVar {
    pub name: String,
    pub base: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Loc {
    pub file: PathBuf,
    pub line: usize,
}

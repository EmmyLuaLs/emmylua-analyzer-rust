use emmylua_parser::{LuaVersionCondition, VisibilityKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaCommonProperty {
    pub id: LuaPropertyId,
    pub description: Option<Box<String>>,
    pub visibility: Option<VisibilityKind>,
    pub source: Option<Box<String>>,
    pub deprecated: Option<LuaDeprecated>,
    pub version_conds: Option<Box<Vec<LuaVersionCondition>>>,
    pub tag_content: Option<Box<LuaTagContent>>,
    pub export: Option<LuaExport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LuaDeprecated {
    Deprecated,
    DeprecatedWithMessage(Box<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LuaExportScope {
    Global,
    Namespace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaTagContent {
    pub tags: Vec<(String, String)>,
}

impl LuaTagContent {
    pub fn new() -> Self {
        Self { tags: Vec::new() }
    }

    pub fn add_tag(&mut self, tag: String, content: String) {
        self.tags.push((tag, content));
    }

    pub fn get_all_tags(&self) -> &[(String, String)] {
        &self.tags
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaExport {
    pub scope: LuaExportScope,
}

impl LuaCommonProperty {
    pub fn new(id: LuaPropertyId) -> Self {
        Self {
            id,
            description: None,
            visibility: None,
            source: None,
            deprecated: None,
            version_conds: None,
            tag_content: None,
            export: None,
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Copy)]
pub struct LuaPropertyId {
    id: u32,
}

impl LuaPropertyId {
    pub fn new(id: u32) -> Self {
        Self { id }
    }
}

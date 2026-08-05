use serde::Serialize;

use super::markdown::render_markdown;
use super::render::html_escape;

/// A navigation entry pointing to a generated page.
#[derive(Debug, Clone, Serialize)]
pub struct NavItem {
    pub name: String,
    pub href: String,
    /// Whether this entry corresponds to the current page.
    #[serde(default)]
    pub active: bool,
}

/// A group of navigation entries sharing a leading character, used to keep the
/// sidebar manageable when there are many items.
#[derive(Debug, Clone, Serialize)]
pub struct NavGroup {
    /// The leading character, or `#` for non-alphabetic names.
    pub letter: String,
    /// Whether the group should start expanded (contains the active item).
    pub open: bool,
    pub items: Vec<NavItem>,
}

/// A node in the module/namespace hierarchy tree used by the sidebar.
///
/// Folders have `href == None` and children; leaves have an `href` and point to
/// a generated page.
#[derive(Debug, Clone, Serialize)]
pub struct NavTreeNode {
    /// Visible label (namespace segment or simple type name).
    pub label: String,
    /// Full search key (e.g. `class lsp.CodeActionKind`); `Some` for leaves.
    pub data_name: Option<String>,
    /// `class` / `enum` / `alias` for leaves, empty for folders.
    pub kind: String,
    pub href: Option<String>,
    #[serde(default)]
    pub active: bool,
    /// Whether this folder starts expanded (on the active branch).
    #[serde(default)]
    pub open: bool,
    pub children: Vec<NavTreeNode>,
}

/// Sidebar navigation model shared by every page.
#[derive(Debug, Clone, Serialize, Default)]
pub struct NavModel {
    pub site_name: String,
    /// `""` on root pages, `"../"` inside subdirectories; prefixes all relative
    /// hrefs (sidebar links, static assets, index link).
    pub root_prefix: String,
    pub types: Vec<NavItem>,
    pub modules: Vec<NavItem>,
    pub globals: Vec<NavItem>,
    pub type_groups: Vec<NavGroup>,
    pub module_groups: Vec<NavGroup>,
    pub global_groups: Vec<NavGroup>,
    /// Module hierarchy tree for types (sidebar).
    pub type_tree: Vec<NavTreeNode>,
    /// Pre-rendered sidebar HTML (module hierarchy tree for types).
    pub sidebar_html: String,
}

/// A parameter or return value row in a function's detail table.
#[derive(Debug, Serialize, Default)]
pub struct HtmlParam {
    pub name: String,
    /// Rendered type HTML (with links).
    pub type_html: String,
    pub description: Option<String>,
}

/// A documented member (method or field).
#[derive(Debug, Serialize, Default)]
pub struct HtmlMember {
    pub name: String,
    /// Rendered `<pre>` code block.
    pub display: String,
    pub description: Option<String>,
    pub deprecated: Option<String>,
    pub see: Option<String>,
    pub other: Option<String>,
    pub params: Vec<HtmlParam>,
    pub returns: Vec<HtmlParam>,
    /// Additional rendered signatures from `---@overload` declarations.
    pub overloads: Vec<String>,
}

/// Data for a single generated page.
#[derive(Debug, Serialize, Default)]
pub struct HtmlDoc {
    /// Item kind: `class` / `enum` / `alias` / `module` / `global`.
    pub kind: String,
    pub name: String,
    pub title: String,
    /// Rendered `<pre>` code block (aliases, simple globals).
    pub display: Option<String>,
    pub supers: Option<String>,
    pub namespace: Option<String>,
    pub description: Option<String>,
    pub deprecated: Option<String>,
    pub see: Option<String>,
    pub other: Option<String>,
    pub fields: Vec<HtmlMember>,
    pub methods: Vec<HtmlMember>,
    pub nav: NavModel,
}

impl HtmlDoc {
    /// Stores rendered doc fields. Description / see / other are rendered as
    /// markdown HTML; deprecated is escaped plain text.
    pub fn set_property(&mut self, property: crate::markdown_generator::markdown_types::Property) {
        self.description = property.description.map(|s| render_markdown(&s));
        self.deprecated = property.deprecated.map(|s| html_escape(&s));
        self.see = property.see.map(|s| render_markdown(&s));
        self.other = property.other.map(|s| render_markdown(&s));
    }

    pub fn set_namespace(&mut self, namespace: &str) {
        self.namespace = Some(html_escape(namespace));
    }

    pub fn set_supers(&mut self, supers: String) {
        self.supers = Some(supers);
    }
}

impl HtmlMember {
    pub fn from_property(
        name: String,
        display: String,
        property: crate::markdown_generator::markdown_types::Property,
    ) -> HtmlMember {
        HtmlMember {
            name,
            display,
            description: property.description.map(|s| render_markdown(&s)),
            deprecated: property.deprecated.map(|s| html_escape(&s)),
            see: property.see.map(|s| render_markdown(&s)),
            other: property.other.map(|s| render_markdown(&s)),
            params: Vec::new(),
            returns: Vec::new(),
            overloads: Vec::new(),
        }
    }
}

use serde::Serialize;

use super::markdown::render_markdown;
use super::render::html_escape;

/// A navigation entry pointing to a generated page.
#[derive(Debug, Clone, Serialize)]
pub struct NavItem {
    pub name: String,
    pub href: String,
    /// Item kind for types (`class` / `enum` / `alias`), empty for
    /// modules/globals. Used for the small colored badge in listings.
    #[serde(default)]
    pub kind: String,
    /// Uppercase first letter of `kind` (`C` / `E` / `A`), empty when `kind`
    /// is empty. Shown on the badge next to the item in listings.
    #[serde(default)]
    pub kind_letter: String,
    /// Display name without the kind prefix. Used by index page listings.
    #[serde(default)]
    pub short_name: String,
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
    /// URL-safe anchor id for the group (letters map to themselves, `#` to `num`).
    #[serde(default)]
    pub anchor: String,
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
    /// Full name including the owner prefix (e.g. `buffer.put`); used as the
    /// in-page anchor id so deep links stay unique.
    pub name: String,
    /// Bare member name (e.g. `put`); shown in the TOC, rustdoc-style.
    pub short_name: String,
    /// Rendered `<pre>` code block.
    pub display: String,
    /// First paragraph of the description (rendered markdown); shown in the
    /// collapsed member header as a one-line summary.
    pub summary: Option<String>,
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
    /// Uppercase first letter of `kind`, shown on the page-header badge.
    pub kind_letter: String,
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
        let short_name = name.rsplit('.').next().unwrap_or(&name).to_string();
        let description = property.description.map(|s| render_markdown(&s));
        // The summary is the rendered first paragraph of the description.
        let summary = description.as_deref().and_then(|html| {
            let end = html.find("</p>").map(|i| i + 4).unwrap_or(0);
            if end > 0 {
                Some(html[..end].to_string())
            } else {
                None
            }
        });
        HtmlMember {
            name,
            short_name,
            display,
            summary,
            description,
            deprecated: property.deprecated.map(|s| html_escape(&s)),
            see: property.see.map(|s| render_markdown(&s)),
            other: property.other.map(|s| render_markdown(&s)),
            params: Vec::new(),
            returns: Vec::new(),
            overloads: Vec::new(),
        }
    }
}

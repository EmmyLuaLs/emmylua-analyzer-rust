mod generators;
mod html_type;
mod init;
mod markdown;
mod render;
mod types;

use std::path::Path;

use emmylua_code_analysis::LuaType;
use tera::Tera;

use crate::OutputDestination;
use crate::doc_model::{DocModel, DocTypeKey};
use types::{HtmlDoc, NavGroup, NavItem, NavModel, NavTreeNode};

use self::generators::{GenContext, build_global_doc, build_module_doc, build_type_doc};

const DEFAULT_SITE_NAME: &str = "Docs";

const KIND_DIRS: [&str; 5] = ["class", "enum", "alias", "module", "global"];

/// Generates a rustdoc-style static HTML site into `output`.
pub fn generate_html(
    analysis: &emmylua_code_analysis::EmmyLuaAnalysis,
    output: OutputDestination,
    site_name: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let OutputDestination::File(output) = output else {
        return Err("Output must be a path when using html format".into());
    };
    std::fs::create_dir_all(&output)?;
    for dir in KIND_DIRS {
        std::fs::create_dir_all(output.join(dir))?;
    }

    let site_name = site_name.unwrap_or_else(|| DEFAULT_SITE_NAME.to_string());
    let tl = init::init_html_tl().ok_or("Failed to initialize HTML templates")?;
    init::write_static_assets(&output)?;

    let model = DocModel::build(analysis);
    let mut nav = NavModel {
        site_name: site_name.clone(),
        ..Default::default()
    };

    // Build a map of documented type full-names -> root-relative page hrefs so
    // signatures can link to them.
    let link_map = build_link_map(&model);

    // Item pages live one level below the root, so links inside signatures use
    // the "../" prefix.
    let sub_linker =
        |id: &DocTypeKey| link_type(&model, &link_map, id).map(|href| format!("../{href}"));
    let ctx = GenContext {
        model: &model,
        linker: &sub_linker,
    };

    let mut pages: Vec<(String, HtmlDoc)> = Vec::new();

    for doc_type in &model.types {
        if let Some(doc) = build_type_doc(&ctx, doc_type) {
            let dir = doc.kind.as_str();
            let filename = format!("{}/{}.html", dir, escape(doc.name.clone()));
            nav.types.push(NavItem {
                name: format!("{} {}", doc.kind, doc.name),
                href: filename.clone(),
                kind: doc.kind.clone(),
                kind_letter: String::new(),
                short_name: doc.name.clone(),
                active: false,
            });
            pages.push((filename, doc));
        }
    }

    for module in &model.modules {
        if let Some(doc) = build_module_doc(&ctx, module) {
            let filename = format!("module/{}.html", escape(doc.name.clone()));
            nav.modules.push(NavItem {
                name: doc.name.clone(),
                href: filename.clone(),
                kind: "module".to_string(),
                kind_letter: String::new(),
                short_name: doc.name.clone(),
                active: false,
            });
            pages.push((filename, doc));
        }
    }

    for global in &model.globals {
        if !global_documented(global) {
            continue;
        }
        if let Some(doc) = build_global_doc(&ctx, global) {
            let filename = format!("global/{}.html", escape(doc.name.clone()));
            nav.globals.push(NavItem {
                name: doc.name.clone(),
                href: filename.clone(),
                kind: "global".to_string(),
                kind_letter: String::new(),
                short_name: doc.name.clone(),
                active: false,
            });
            pages.push((filename, doc));
        }
    }

    sort_nav(&mut nav);
    fill_kind_letters(&mut nav);

    // Render item pages (in subdirectories -> "../" prefix).
    for (filename, mut doc) in pages {
        doc.kind_letter = doc
            .kind
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase().to_string())
            .unwrap_or_else(|| "?".to_string());
        let mut sub_nav = nav.clone();
        sub_nav.root_prefix = "../".to_string();
        mark_active(&mut sub_nav, &doc.kind, &doc.name);
        finalize_nav(&mut sub_nav);
        doc.nav = sub_nav;
        let html = render_page(&tl, "item.html", &doc)?;
        std::fs::write(output.join(&filename), html)?;
    }

    // Landing page at the root.
    let mut index_nav = nav.clone();
    finalize_nav(&mut index_nav);
    let index_doc = HtmlDoc {
        title: "index".to_string(),
        name: site_name.clone(),
        nav: index_nav,
        ..Default::default()
    };
    let index_html = render_page(&tl, "index.html", &index_doc)?;
    std::fs::write(output.join("index.html"), index_html)?;

    write_search_index(&output, &index_doc.nav)?;

    eprintln!("Documentation html exported to {:?}", output);
    Ok(())
}

/// Returns a root-relative href for `id` if its type is documented.
fn link_type(
    model: &DocModel,
    link_map: &std::collections::HashMap<String, String>,
    id: &DocTypeKey,
) -> Option<String> {
    let name = model.type_name(&id.to_lua_id());
    link_map.get(&name).cloned()
}

/// Maps every documented type full-name to its root-relative page href.
fn build_link_map(model: &DocModel) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for doc_type in &model.types {
        map.insert(
            doc_type.full_name.clone(),
            format!(
                "{}/{}.html",
                doc_type.kind.as_str(),
                escape(doc_type.full_name.clone())
            ),
        );
    }
    map
}

fn global_documented(global: &crate::doc_model::DocGlobal) -> bool {
    !matches!(global.ty, LuaType::Ref(_) | LuaType::Def(_))
}

fn sort_nav(nav: &mut NavModel) {
    nav.types.sort_by(|a, b| a.name.cmp(&b.name));
    nav.modules.sort_by(|a, b| a.name.cmp(&b.name));
    nav.globals.sort_by(|a, b| a.name.cmp(&b.name));
}

/// Fills the `kind_letter` badge field on every nav item.
fn fill_kind_letters(nav: &mut NavModel) {
    for item in nav
        .types
        .iter_mut()
        .chain(nav.modules.iter_mut())
        .chain(nav.globals.iter_mut())
    {
        item.kind_letter = item
            .kind
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase().to_string())
            .unwrap_or_default();
    }
}

/// Marks the nav entry matching the current page as active.
fn mark_active(nav: &mut NavModel, kind: &str, name: &str) {
    let target = match kind {
        "module" | "global" => name.to_string(),
        _ => format!("{kind} {name}"),
    };
    for item in nav
        .types
        .iter_mut()
        .chain(nav.modules.iter_mut())
        .chain(nav.globals.iter_mut())
    {
        if item.name == target {
            item.active = true;
            return;
        }
    }
}

/// Prepares the nav for rendering: letter groups (index page), the module
/// hierarchy tree and the pre-rendered sidebar HTML.
fn finalize_nav(nav: &mut NavModel) {
    nav.type_groups = build_groups(&nav.types);
    nav.module_groups = build_groups(&nav.modules);
    nav.global_groups = build_groups(&nav.globals);
    nav.type_tree = build_type_tree(&nav.types);
    nav.sidebar_html = build_sidebar_html(nav);
}

fn build_groups(items: &[NavItem]) -> Vec<NavGroup> {
    let mut groups: Vec<(String, Vec<NavItem>)> = Vec::new();
    for item in items {
        let letter = group_key(&item.name);
        match groups.iter_mut().find(|(l, _)| *l == letter) {
            Some((_, list)) => list.push(item.clone()),
            None => groups.push((letter, vec![item.clone()])),
        }
    }
    groups
        .into_iter()
        .map(|(letter, group_items)| {
            let open = group_items.iter().any(|item| item.active);
            let anchor = if letter == "#" {
                "num".to_string()
            } else {
                letter.clone()
            };
            NavGroup {
                letter,
                anchor,
                open,
                items: group_items,
            }
        })
        .collect()
}

/// The leading character used to group a sidebar entry. Type entries carry a
/// `class ` / `enum ` / `alias ` prefix in their display name; the group letter
/// is taken from the actual item name, not the prefix.
fn group_key(name: &str) -> String {
    let bare = name
        .strip_prefix("class ")
        .or_else(|| name.strip_prefix("enum "))
        .or_else(|| name.strip_prefix("alias "))
        .unwrap_or(name);
    bare.chars()
        .next()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase().to_string())
        .unwrap_or_else(|| "#".to_string())
}

fn escape(name: String) -> String {
    super::markdown_generator::escape_type_name(&name)
}

// ─── Module hierarchy tree ───────────────────────────────────────────────

/// Builds the module/namespace hierarchy tree from the flat type nav items.
///
/// Each item name is `"class lsp.CodeActionKind"`; the namespace path
/// (`lsp`) becomes folders and the final segment a leaf.
fn build_type_tree(items: &[NavItem]) -> Vec<NavTreeNode> {
    let mut roots: Vec<NavTreeNode> = Vec::new();
    for item in items {
        let (kind, full_name) = split_type_name(&item.name);
        let segments: Vec<&str> = full_name.split('.').collect();
        insert_tree(&mut roots, &segments, item, kind);
    }
    sort_tree(&mut roots);
    set_open(&mut roots);
    roots
}

fn split_type_name(name: &str) -> (&str, &str) {
    for prefix in ["class ", "enum ", "alias "] {
        if let Some(rest) = name.strip_prefix(prefix) {
            return (prefix.trim(), rest);
        }
    }
    ("class", name)
}

fn insert_tree(nodes: &mut Vec<NavTreeNode>, path: &[&str], item: &NavItem, kind: &str) {
    let head = path[0];
    if path.len() == 1 {
        nodes.push(NavTreeNode {
            label: head.to_string(),
            data_name: Some(item.name.clone()),
            kind: kind.to_string(),
            href: Some(item.href.clone()),
            active: item.active,
            open: false,
            children: Vec::new(),
        });
        return;
    }
    let idx = match nodes
        .iter()
        .position(|n| n.href.is_none() && n.label == head)
    {
        Some(idx) => idx,
        None => {
            nodes.push(NavTreeNode {
                label: head.to_string(),
                data_name: None,
                kind: String::new(),
                href: None,
                active: false,
                open: false,
                children: Vec::new(),
            });
            nodes.len() - 1
        }
    };
    insert_tree(&mut nodes[idx].children, &path[1..], item, kind);
}

fn sort_tree(nodes: &mut [NavTreeNode]) {
    nodes.sort_by(|a, b| a.label.cmp(&b.label));
    for node in nodes.iter_mut() {
        sort_tree(&mut node.children);
    }
}

/// Marks folders on the active branch as open. Returns whether `nodes` contain
/// an active item.
fn set_open(nodes: &mut [NavTreeNode]) -> bool {
    let mut has_active = false;
    for node in nodes.iter_mut() {
        if node.active {
            has_active = true;
        } else if set_open(&mut node.children) {
            node.open = true;
            has_active = true;
        }
    }
    has_active
}

/// Renders the full sidebar: the type hierarchy tree plus flat module/global
/// lists. Kept in Rust (not the template) so the tree can recurse freely.
fn build_sidebar_html(nav: &NavModel) -> String {
    let mut html = String::new();
    if !nav.type_tree.is_empty() {
        html.push_str(&category_header("Types", nav.types.len()));
        html.push_str(&render_tree_html(&nav.type_tree, &nav.root_prefix));
        html.push_str("</ul></div>");
    }
    if !nav.modules.is_empty() {
        html.push_str(&category_header("Modules", nav.modules.len()));
        for item in &nav.modules {
            html.push_str(&render_flat_item(item, &nav.root_prefix));
        }
        html.push_str("</ul></div>");
    }
    if !nav.globals.is_empty() {
        html.push_str(&category_header("Globals", nav.globals.len()));
        for item in &nav.globals {
            html.push_str(&render_flat_item(item, &nav.root_prefix));
        }
        html.push_str("</ul></div>");
    }
    html
}

/// Opens a sidebar category section: title plus an item count badge.
fn category_header(title: &str, count: usize) -> String {
    format!(
        "<div class=\"sidebar-category\"><div class=\"category-title\">{title}<span class=\"category-count\">{count}</span></div><ul>"
    )
}

/// Single-letter badge rendered next to a type leaf in the sidebar.
fn kind_badge(kind: &str) -> &'static str {
    match kind {
        "class" => "C",
        "enum" => "E",
        "alias" => "A",
        _ => "T",
    }
}

fn render_flat_item(item: &NavItem, root_prefix: &str) -> String {
    let active_cls = if item.active { " active" } else { "" };
    format!(
        "<li><a data-name=\"{}\" href=\"{}{}\" class=\"nav-item{active_cls}\">{}</a></li>",
        render::html_escape(&item.name),
        root_prefix,
        render::html_escape(&item.href),
        render::html_escape(&item.name)
    )
}

fn render_tree_html(nodes: &[NavTreeNode], root_prefix: &str) -> String {
    let mut html = String::new();
    for node in nodes {
        if let Some(href) = &node.href {
            let active_cls = if node.active { " active" } else { "" };
            let data = node.data_name.as_deref().unwrap_or(&node.label);
            let badge = kind_badge(&node.kind);
            html.push_str(&format!(
                "<li><a data-name=\"{}\" href=\"{}{}\" class=\"nav-item{active_cls}\"><span class=\"kind-badge kind-{}\" aria-hidden=\"true\">{}</span><span class=\"nav-label\">{}</span></a></li>",
                render::html_escape(data),
                root_prefix,
                render::html_escape(href),
                render::html_escape(&node.kind),
                badge,
                render::html_escape(&node.label)
            ));
        } else {
            let open = if node.open { " open" } else { "" };
            html.push_str(&format!(
                "<li><details{open}><summary class=\"group-label\"><span class=\"group-caret\" aria-hidden=\"true\"></span><span class=\"nav-label\">{}</span></summary><ul>",
                render::html_escape(&node.label)
            ));
            html.push_str(&render_tree_html(&node.children, root_prefix));
            html.push_str("</ul></details></li>");
        }
    }
    html
}

fn render_page(
    tl: &Tera,
    template: &str,
    doc: &HtmlDoc,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut context = tera::Context::new();
    context.insert("doc", doc);
    context.insert("site_name", &doc.nav.site_name);
    context.insert("nav", &doc.nav);
    context.insert("root_prefix", &doc.nav.root_prefix);
    tl.render(template, &context).map_err(Into::into)
}

fn write_search_index(output: &Path, nav: &NavModel) -> Result<(), Box<dyn std::error::Error>> {
    let mut entries = Vec::new();
    for item in &nav.types {
        entries.push(serde_json::json!({
            "name": item.name,
            "href": item.href,
            "kind": type_kind(&item.name),
        }));
    }
    for item in &nav.modules {
        entries.push(serde_json::json!({
            "name": item.name,
            "href": item.href,
            "kind": "module",
        }));
    }
    for item in &nav.globals {
        entries.push(serde_json::json!({
            "name": item.name,
            "href": item.href,
            "kind": "global",
        }));
    }
    let json = serde_json::to_string(&entries)?;
    std::fs::write(
        output.join("static").join("search-index.js"),
        format!("window.SEARCH_INDEX = {json};"),
    )?;
    Ok(())
}

fn type_kind(name: &str) -> &str {
    if name.starts_with("enum ") {
        "enum"
    } else if name.starts_with("alias ") {
        "alias"
    } else {
        "class"
    }
}

mod generators;
mod html_type;
mod init;
mod markdown;
mod render;
mod types;

use std::path::Path;

use emmylua_code_analysis::{DbIndex, LuaDeclId, LuaTypeDeclId};
use tera::Tera;

use crate::OutputDestination;
use types::{HtmlDoc, NavGroup, NavItem, NavModel};

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

    let db = analysis.compilation.get_db();
    let mut nav = NavModel {
        site_name: site_name.clone(),
        ..Default::default()
    };

    // Build a map of documented type full-names -> root-relative page hrefs so
    // signatures can link to them.
    let link_map = build_link_map(db);

    // Item pages live one level below the root, so links inside signatures use
    // the "../" prefix.
    let sub_linker =
        |id: &LuaTypeDeclId| link_type(db, &link_map, id).map(|href| format!("../{href}"));
    let ctx = GenContext {
        db,
        linker: &sub_linker,
    };

    let mut pages: Vec<(String, HtmlDoc)> = Vec::new();

    let type_index = db.get_type_index();
    for typ in type_index.get_all_types() {
        if !type_in_main_workspace(db, typ.get_locations()) {
            continue;
        }
        if let Some(doc) = build_type_doc(&ctx, typ) {
            let dir = doc.kind.as_str();
            let filename = format!("{}/{}.html", dir, escape(doc.name.clone()));
            nav.types.push(NavItem {
                name: format!("{} {}", doc.kind, doc.name),
                href: filename.clone(),
                active: false,
            });
            pages.push((filename, doc));
        }
    }

    let module_index = db.get_module_index();
    for module in module_index.get_module_infos() {
        let Some(workspace) = db.get_module_index().get_module(module.file_id) else {
            continue;
        };
        if !workspace.workspace_id.is_main() {
            continue;
        }
        if let Some(doc) = build_module_doc(&ctx, module) {
            let filename = format!("module/{}.html", escape(doc.name.clone()));
            nav.modules.push(NavItem {
                name: doc.name.clone(),
                href: filename.clone(),
                active: false,
            });
            pages.push((filename, doc));
        }
    }

    let global_index = db.get_global_index();
    for decl_id in global_index.get_all_global_decl_ids() {
        if !global_in_main_workspace(db, &decl_id) {
            continue;
        }
        if let Some(doc) = build_global_doc(&ctx, &decl_id) {
            let filename = format!("global/{}.html", escape(doc.name.clone()));
            nav.globals.push(NavItem {
                name: doc.name.clone(),
                href: filename.clone(),
                active: false,
            });
            pages.push((filename, doc));
        }
    }

    sort_nav(&mut nav);

    // Render item pages (in subdirectories -> "../" prefix).
    for (filename, mut doc) in pages {
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
    db: &DbIndex,
    link_map: &std::collections::HashMap<String, String>,
    id: &LuaTypeDeclId,
) -> Option<String> {
    let name = db.get_type_index().get_type_decl(id)?.get_full_name();
    link_map.get(name).cloned()
}

/// Maps every documented type full-name to its root-relative page href.
fn build_link_map(db: &DbIndex) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let type_index = db.get_type_index();
    for typ in type_index.get_all_types() {
        if !type_in_main_workspace(db, typ.get_locations()) {
            continue;
        }
        let kind = if typ.is_class() {
            "class"
        } else if typ.is_enum() {
            "enum"
        } else {
            "alias"
        };
        let full_name = typ.get_full_name().to_string();
        map.insert(
            full_name.clone(),
            format!("{}/{}.html", kind, escape(full_name)),
        );
    }
    map
}

fn type_in_main_workspace(
    db: &DbIndex,
    locations: &[emmylua_code_analysis::LuaDeclLocation],
) -> bool {
    locations.iter().any(|loc| {
        db.get_module_index()
            .get_module(loc.file_id)
            .is_some_and(|module| module.workspace_id.is_main())
    })
}

fn global_in_main_workspace(db: &DbIndex, decl_id: &LuaDeclId) -> bool {
    let Some(module) = db.get_module_index().get_module(decl_id.file_id) else {
        return false;
    };
    if !module.workspace_id.is_main() {
        return false;
    }
    let Some(decl_type) = db.get_type_index().get_type_cache(&(*decl_id).into()) else {
        return false;
    };
    !matches!(
        decl_type.as_type(),
        emmylua_code_analysis::LuaType::Ref(_) | emmylua_code_analysis::LuaType::Def(_)
    )
}

fn sort_nav(nav: &mut NavModel) {
    nav.types.sort_by(|a, b| a.name.cmp(&b.name));
    nav.modules.sort_by(|a, b| a.name.cmp(&b.name));
    nav.globals.sort_by(|a, b| a.name.cmp(&b.name));
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

/// Splits each nav list into letter groups (used by the sidebar).
fn finalize_nav(nav: &mut NavModel) {
    nav.type_groups = build_groups(&nav.types);
    nav.module_groups = build_groups(&nav.modules);
    nav.global_groups = build_groups(&nav.globals);
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
            NavGroup {
                letter,
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

use std::path::Path;

use emmylua_code_analysis::{
    humanize_type, DbIndex, LuaDecl, LuaDeclId, LuaMemberOwner, LuaSemanticDeclId, LuaType,
    RenderLevel,
};
use tera::Tera;

use crate::markdown_generator::{
    escape_type_name,
    gen::mod_gen::generate_member_owner_module,
    markdown_types::{Doc, IndexStruct, MkdocsIndex},
    render::{render_const_type, render_function_type},
};

use super::collect_property;

pub fn generate_global_markdown(
    db: &DbIndex,
    tl: &Tera,
    decl_id: &LuaDeclId,
    output: &Path,
    mkdocs_index: &mut MkdocsIndex,
) -> Option<()> {
    check_filter(db, decl_id)?;

    let mut context = tera::Context::new();
    let mut doc = Doc::default();

    let decl = db.get_decl_index().get_decl(decl_id)?;
    let name = decl.get_name();
    doc.name = name.to_string();
    doc.property = collect_property(db, LuaSemanticDeclId::LuaDecl(decl.get_id()));

    let decl_type = db.get_type_index().get_type_cache(&(*decl_id).into())?;
    let mut template_name = "lua_global_template.tl";
    match decl_type.as_type() {
        LuaType::TableConst(table) => {
            let member_owner = LuaMemberOwner::Element(table.clone());
            generate_member_owner_module(db, member_owner, name, &mut doc)?;
        }
        _ => {
            template_name = "lua_global_template_simple.tl";
            generate_simple_global(db, decl, &mut doc);
        }
    }
    context.insert("doc", &doc);

    let render_text = match tl.render(template_name, &context) {
        Ok(text) => text,
        Err(e) => {
            log::error!("Failed to render template: {}", e);
            return None;
        }
    };

    let file_name = format!("{}.md", escape_type_name(decl.get_name()));
    mkdocs_index.globals.push(IndexStruct {
        name: decl.get_name().to_string(),
        file: format!("globals/{}", file_name.clone()),
    });

    let outpath = output.join(file_name);
    log::info!("Writing global file: {}", outpath.display());
    match std::fs::write(outpath, render_text) {
        Ok(_) => {}
        Err(e) => {
            log::error!("Failed to write file: {}", e);
            return None;
        }
    }
    Some(())
}

fn check_filter(db: &DbIndex, decl_id: &LuaDeclId) -> Option<()> {
    let file_id = decl_id.file_id;
    let module = db.get_module_index().get_module(file_id)?;
    if !module.workspace_id.is_main() {
        return None;
    };
    let decl_type = db.get_type_index().get_type_cache(&(*decl_id).into())?;
    match decl_type.as_type() {
        LuaType::Ref(_) | LuaType::Def(_) => return None,
        _ => {}
    }

    Some(())
}

fn generate_simple_global(db: &DbIndex, decl: &LuaDecl, doc: &mut Doc) -> Option<()> {
    let semantic_decl = LuaSemanticDeclId::LuaDecl(decl.get_id());
    doc.property = collect_property(db, semantic_decl);

    let name = decl.get_name();
    let ty = db.get_type_index().get_type_cache(&decl.get_id().into())?;
    if ty.is_function() {
        let display = render_function_type(db, ty, name, false);
        doc.display = Some(display);
    } else if ty.is_const() {
        let typ_display = render_const_type(db, ty);
        let display = format!("```lua\n{}: {}\n```\n", name, typ_display);
        doc.display = Some(display);
    } else {
        let typ_display = humanize_type(db, ty, RenderLevel::Detailed);
        let display = format!("```lua\n{} : {}\n```\n", name, typ_display);
        doc.display = Some(display);
    }

    Some(())
}

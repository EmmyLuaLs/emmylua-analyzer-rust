use std::path::Path;

use emmylua_code_analysis::{
    humanize_type, DbIndex, LuaDecl, LuaDeclId, LuaMemberOwner, LuaPropertyOwnerId, LuaType,
    RenderLevel,
};
use tera::{Context, Tera};

use crate::markdown_generator::{
    escape_type_name, mod_gen::generate_member_owner_module, IndexStruct,
};

use super::{
    render::{render_const_type, render_function_type},
    MkdocsIndex,
};

pub fn generate_global_markdown(
    db: &DbIndex,
    tl: &Tera,
    decl_id: &LuaDeclId,
    output: &Path,
    mkdocs_index: &mut MkdocsIndex,
) -> Option<()> {
    check_filter(db, decl_id)?;

    let mut context = tera::Context::new();
    let decl = db.get_decl_index().get_decl(decl_id)?;
    let name = decl.get_name();
    context.insert("global_name", name);

    let mut template_name = "lua_global_template.tl";
    match &decl.get_type()? {
        LuaType::TableConst(table) => {
            let member_owner = LuaMemberOwner::Element(table.clone());
            generate_member_owner_module(db, member_owner, name, &mut context)?;
        }
        _ => {
            template_name = "lua_global_template_simple.tl";
            generate_simple_global(db, decl, &mut context);
        }
    }

    let render_text = match tl.render(&template_name, &context) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("Failed to render template: {}", e);
            return None;
        }
    };

    let file_name = format!("{}.md", escape_type_name(&decl.get_name()));
    mkdocs_index.globals.push(IndexStruct {
        name: decl.get_name().to_string(),
        file: format!("globals/{}", file_name.clone()),
    });

    let outpath = output.join(file_name);
    println!("output global file: {}", outpath.display());
    match std::fs::write(outpath, render_text) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Failed to write file: {}", e);
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
    }

    let decl = db.get_decl_index().get_decl(decl_id)?;
    let ty = decl.get_type()?;
    match ty {
        LuaType::Ref(_) | LuaType::Def(_) => return None,
        _ => {}
    }

    Some(())
}

fn generate_simple_global(db: &DbIndex, decl: &LuaDecl, context: &mut Context) -> Option<()> {
    let property_owner_id = LuaPropertyOwnerId::LuaDecl(decl.get_id());
    let property = db.get_property_index().get_property(property_owner_id);

    let description = if let Some(property) = property {
        *property
            .description
            .clone()
            .unwrap_or("".to_string().into())
    } else {
        "".to_string()
    };
    context.insert("description", &description);

    let name = decl.get_name();
    let ty = decl.get_type().unwrap_or(&LuaType::Unknown);
    if ty.is_function() {
        let display = render_function_type(db, ty, &name, false);
        context.insert("display", &display);
    } else if ty.is_const() {
        let typ_display = render_const_type(db, &ty);
        let display = format!("```lua\n{}: {}\n```\n", name, typ_display);
        context.insert("display", &display);
    } else {
        let typ_display = humanize_type(db, &ty, RenderLevel::Detailed);
        let display = format!("```lua\n{} : {}\n```\n", name, typ_display);
        context.insert("display", &display);
    }

    Some(())
}

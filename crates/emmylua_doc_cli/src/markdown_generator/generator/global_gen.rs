use std::path::Path;

use crate::doc_model::{DocGlobal, DocModel};
use crate::markdown_generator::{
    escape_type_name,
    generator::typ_gen::collect_owner_members,
    markdown_types::{Doc, IndexStruct, MkdocsIndex},
    render::{render_const_type, render_function_type},
};
use emmylua_code_analysis::{LuaType, RenderLevel};
use tera::Tera;

use super::collect_property;

pub fn generate_global_markdown(
    model: &DocModel,
    tl: &Tera,
    global: &DocGlobal,
    output: &Path,
    mkdocs_index: &mut MkdocsIndex,
) -> Option<()> {
    // Runtime class values (`local Animal = {}`) are not output as global doc pages.
    if matches!(global.ty, LuaType::Ref(_) | LuaType::Def(_)) {
        return None;
    }

    let mut context = tera::Context::new();
    let mut doc = Doc {
        name: global.name.clone(),
        property: collect_property(&global.property),
        ..Default::default()
    };
    let mut template_name = "lua_global_template.tl";
    match &global.ty {
        LuaType::TableConst(_) | LuaType::Instance(_) => {
            let members = model.members_of_type(&global.ty);
            let (methods, fields) = collect_owner_members(model, &members, &global.name);
            doc.methods = if methods.is_empty() {
                None
            } else {
                Some(methods)
            };
            doc.fields = if fields.is_empty() {
                None
            } else {
                Some(fields)
            };
        }
        _ => {
            template_name = "lua_global_template_simple.tl";
            generate_simple_global(model, &mut doc, global);
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

    let file_name = format!("{}.md", escape_type_name(&global.name));
    mkdocs_index.globals.push(IndexStruct {
        name: global.name.clone(),
        file: format!("globals/{}", file_name.clone()),
    });

    let outpath = output.join(file_name);
    log::info!("Writing global file: {}", outpath.display());
    std::fs::write(outpath, render_text).ok()?;
    Some(())
}

fn generate_simple_global(model: &DocModel, doc: &mut Doc, global: &DocGlobal) {
    doc.property = collect_property(&global.property);
    let name = &global.name;
    if model
        .function_info(&global.ty, global.signature.as_ref())
        .is_some()
    {
        doc.display = Some(render_function_type(model, &global.ty, name, false));
    } else if global.ty.is_const() {
        let typ_display = render_const_type(model, &global.ty);
        doc.display = Some(format!("```lua\n{name}: {typ_display}\n```\n"));
    } else {
        let typ_display = model.render_type(&global.ty, RenderLevel::Detailed);
        doc.display = Some(format!("```lua\n{name} : {typ_display}\n```\n"));
    }
}

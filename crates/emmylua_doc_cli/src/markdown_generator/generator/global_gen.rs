use std::path::Path;

use crate::doc_links::{DocLinkIndex, DocPageRef, LinkContext};
use crate::doc_model::{DocGlobal, DocModel};
use crate::markdown_generator::{
    escape_type_name,
    generator::typ_gen::collect_owner_members,
    markdown_types::{Doc, IndexStruct, MkdocsIndex},
    render::{function_details_md, function_overloads_md, render_const_type, render_function_type},
};
use emmylua_code_analysis::{LuaType, RenderLevel};
use tera::Tera;

use super::collect_property_with_links;

pub fn generate_global_markdown(
    model: &DocModel,
    tl: &Tera,
    global: &DocGlobal,
    output: &Path,
    mkdocs_index: &mut MkdocsIndex,
    doc_links: &DocLinkIndex,
) -> Option<()> {
    // Runtime class values (`local Animal = {}`) are not output as global doc pages.
    if matches!(global.ty, LuaType::Ref(_) | LuaType::Def(_)) {
        return None;
    }

    let mut context = tera::Context::new();
    let links = LinkContext::new(doc_links, DocPageRef::for_global(&global.name), "");
    let mut doc = Doc {
        name: global.name.clone(),
        property: collect_property_with_links(&global.property, Some(&links)),
        ..Default::default()
    };
    let mut template_name = "lua_global_template.tl";
    match &global.ty {
        LuaType::TableConst(_) | LuaType::Instance(_) => {
            let members = model.members_for_global(global);
            let (methods, fields) =
                collect_owner_members(model, &members, &global.name, Some(&links));
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
            generate_simple_global(model, &mut doc, global, &links);
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

fn generate_simple_global(
    model: &DocModel,
    doc: &mut Doc,
    global: &DocGlobal,
    links: &LinkContext,
) {
    doc.property = collect_property_with_links(&global.property, Some(links));
    let name = &global.name;
    if model
        .function_info(&global.ty, global.signature.as_ref())
        .is_some()
    {
        doc.display = Some(render_function_type(
            model,
            &global.ty,
            global.signature.as_ref(),
            name,
            false,
        ));
        let (params, returns) =
            function_details_md(model, &global.ty, global.signature.as_ref(), Some(links))
                .unwrap_or_default();
        doc.params = params;
        doc.returns = returns;
        doc.overloads = function_overloads_md(model, global.signature.as_ref(), name);
    } else if global.ty.is_const() {
        let typ_display = render_const_type(model, &global.ty);
        doc.display = Some(format!("```lua\n{name}: {typ_display}\n```\n"));
    } else {
        let typ_display = model.render_type(&global.ty, RenderLevel::Detailed);
        doc.display = Some(format!("```lua\n{name} : {typ_display}\n```\n"));
    }
}

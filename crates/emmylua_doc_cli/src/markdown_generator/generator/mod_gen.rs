use std::path::Path;

use crate::doc_model::{DocModel, DocModule};
use crate::markdown_generator::{
    escape_type_name,
    generator::typ_gen::collect_owner_members,
    markdown_types::{Doc, IndexStruct, MkdocsIndex},
};
use tera::Tera;

use super::collect_property;

pub fn generate_module_markdown(
    model: &DocModel,
    tl: &Tera,
    module: &DocModule,
    output: &Path,
    mkdocs_index: &mut MkdocsIndex,
) -> Option<()> {
    let mut context = tera::Context::new();
    let mut doc = Doc {
        name: module.name.clone(),
        ..Default::default()
    };
    doc.property = collect_property(&module.property);

    let members = model.members_of_type(&module.export_type);
    if !members.is_empty() {
        let (methods, fields) = collect_owner_members(model, &members, "M");
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

    context.insert("doc", &doc);

    let render_text = match tl.render("lua_module_template.tl", &context) {
        Ok(text) => text,
        Err(e) => {
            log::error!("Failed to render template: {}", e);
            return None;
        }
    };

    let file_name = format!("{}.md", escape_type_name(&module.name));
    mkdocs_index.modules.push(IndexStruct {
        name: module.name.clone(),
        file: format!("modules/{}", file_name.clone()),
    });

    let outpath = output.join(file_name);
    log::info!("Writing module file: {}", outpath.display());
    std::fs::write(outpath, render_text).ok()?;
    Some(())
}

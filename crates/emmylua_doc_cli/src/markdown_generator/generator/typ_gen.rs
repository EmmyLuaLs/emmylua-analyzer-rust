use crate::doc_model::{DocMember, DocModel, DocType};
use crate::markdown_generator::{
    escape_type_name,
    generator::collect_property,
    markdown_types::{Doc, IndexStruct, MemberDoc, MkdocsIndex},
    render::{render_const_type, render_function_type},
};
use emmylua_code_analysis::RenderLevel;
use std::path::Path;
use tera::{Context, Tera};

pub fn generate_type_markdown(
    model: &DocModel,
    tl: &Tera,
    doc_type: &DocType,
    output: &Path,
    mkdocs_index: &mut MkdocsIndex,
) -> Option<()> {
    let mut context = tera::Context::new();
    let typ_name = doc_type.name.clone();
    let mut doc = Doc {
        name: typ_name,
        ..Default::default()
    };

    match doc_type.kind {
        crate::doc_model::DocTypeKind::Class => {
            generate_class_type_markdown(
                model,
                tl,
                doc_type,
                &mut doc,
                &mut context,
                output,
                mkdocs_index,
            );
        }
        crate::doc_model::DocTypeKind::Enum => {
            generate_enum_type_markdown(
                model,
                tl,
                doc_type,
                &mut doc,
                &mut context,
                output,
                mkdocs_index,
            );
        }
        crate::doc_model::DocTypeKind::Alias => {
            generate_alias_type_markdown(
                model,
                tl,
                doc_type,
                &mut doc,
                &mut context,
                output,
                mkdocs_index,
            );
        }
    }
    Some(())
}

fn generate_class_type_markdown(
    model: &DocModel,
    tl: &Tera,
    doc_type: &DocType,
    doc: &mut Doc,
    context: &mut Context,
    output: &Path,
    mkdocs_index: &mut MkdocsIndex,
) -> Option<()> {
    let typ_name = doc_type.name.clone();
    if let Some(index) = doc_type.full_name.rfind('.') {
        doc.namespace = Some(doc_type.full_name[..index].to_string());
    }
    doc.property = collect_property(&doc_type.property);

    if !doc_type.bases.is_empty() {
        let super_type_texts: Vec<String> = doc_type
            .bases
            .iter()
            .map(|ty| model.render_type(ty, RenderLevel::Simple))
            .collect();
        doc.supers = Some(super_type_texts.join(", "));
    }

    let (methods, fields) = collect_owner_members(model, &doc_type.members, &typ_name);
    if !methods.is_empty() {
        doc.methods = Some(methods);
    }
    if !fields.is_empty() {
        doc.fields = Some(fields);
    }

    context.insert("doc", &doc);
    let render_text = match tl.render("lua_type_template.tl", context) {
        Ok(text) => text,
        Err(e) => {
            log::error!("Failed to render template: {}", e);
            return None;
        }
    };

    let file_type_name = format!("{}.md", escape_type_name(&doc_type.full_name));
    mkdocs_index.types.push(IndexStruct {
        name: format!("class {}", typ_name),
        file: format!("types/{}", file_type_name.clone()),
    });

    let outpath = output.join(file_type_name);
    log::info!("Writing class file: {}", outpath.display());
    std::fs::write(outpath, render_text).ok()?;
    Some(())
}

fn generate_enum_type_markdown(
    model: &DocModel,
    tl: &Tera,
    doc_type: &DocType,
    doc: &mut Doc,
    context: &mut Context,
    output: &Path,
    mkdocs_index: &mut MkdocsIndex,
) -> Option<()> {
    let typ_name = doc_type.name.clone();
    if let Some(index) = doc_type.full_name.rfind('.') {
        doc.namespace = Some(doc_type.full_name[..index].to_string());
    }
    doc.property = collect_property(&doc_type.property);

    let field_members: Vec<MemberDoc> = doc_type
        .members
        .iter()
        .map(|member| MemberDoc {
            name: member.name.clone(),
            display: model.render_type(&member.ty, RenderLevel::Simple),
            property: collect_property(&member.property),
        })
        .collect();
    if !field_members.is_empty() {
        doc.fields = Some(field_members);
    }

    context.insert("doc", &doc);
    let render_text = match tl.render("lua_enum_template.tl", context) {
        Ok(text) => text,
        Err(e) => {
            log::error!("Failed to render template: {}", e);
            return None;
        }
    };

    let file_type_name = format!("{}.md", escape_type_name(&doc_type.full_name));
    mkdocs_index.types.push(IndexStruct {
        name: format!("enum {}", typ_name),
        file: format!("types/{}", file_type_name.clone()),
    });

    let outpath = output.join(file_type_name);
    log::info!("Writing enum file: {}", outpath.display());
    std::fs::write(outpath, render_text).ok()?;
    Some(())
}

fn generate_alias_type_markdown(
    model: &DocModel,
    tl: &Tera,
    doc_type: &DocType,
    doc: &mut Doc,
    context: &mut Context,
    output: &Path,
    mkdocs_index: &mut MkdocsIndex,
) -> Option<()> {
    let typ_name = doc_type.name.clone();
    if let Some(index) = doc_type.full_name.rfind('.') {
        doc.namespace = Some(doc_type.full_name[..index].to_string());
    }
    doc.property = collect_property(&doc_type.property);

    if let Some(origin_typ) = &doc_type.alias_type {
        let origin_type_display = model.render_type(origin_typ, RenderLevel::Documentation);
        doc.display = Some(format!(
            "```lua\n(alias) {} = {}\n```\n",
            typ_name, origin_type_display
        ));
    }

    context.insert("doc", &doc);
    let render_text = match tl.render("lua_alias_template.tl", context) {
        Ok(text) => text,
        Err(e) => {
            log::error!("Failed to render template: {}", e);
            return None;
        }
    };

    let file_type_name = format!("{}.md", escape_type_name(&doc_type.full_name));
    mkdocs_index.types.push(IndexStruct {
        name: format!("alias {}", typ_name),
        file: format!("types/{}", file_type_name.clone()),
    });

    let outpath = output.join(file_type_name);
    log::info!("Writing alias file: {}", outpath.display());
    std::fs::write(outpath, render_text).ok()?;
    Some(())
}

pub(crate) fn collect_owner_members(
    model: &DocModel,
    members: &[DocMember],
    owner_name: &str,
) -> (Vec<MemberDoc>, Vec<MemberDoc>) {
    let mut method_members: Vec<MemberDoc> = Vec::new();
    let mut field_members: Vec<MemberDoc> = Vec::new();

    for member in members {
        if member
            .property
            .visibility
            .is_some_and(|visibility| visibility != emmylua_parser::VisibilityKind::Public)
        {
            continue;
        }

        let title_name = format!("{}.{}", owner_name, member.name);
        let property = collect_property(&member.property);
        if model
            .function_info(&member.ty, member.signature.as_ref())
            .is_some()
        {
            let display = render_function_type(model, &member.ty, &title_name, false);
            method_members.push(MemberDoc {
                name: title_name,
                display,
                property,
            });
        } else if member.ty.is_const() {
            let display = render_const_type(model, &member.ty);
            field_members.push(MemberDoc {
                name: title_name.clone(),
                display: format!("```lua\n{}.{}: {}\n```\n", owner_name, member.name, display),
                property,
            });
        } else {
            let typ_display = model.render_type(&member.ty, RenderLevel::Detailed);
            field_members.push(MemberDoc {
                name: title_name.clone(),
                display: format!(
                    "```lua\n{}.{} : {}\n```\n",
                    owner_name, member.name, typ_display
                ),
                property,
            });
        }
    }

    (method_members, field_members)
}

use emmylua_code_analysis::{
    humanize_type, DbIndex, LuaMemberKey, LuaMemberOwner, LuaSemanticDeclId, LuaType, LuaTypeCache,
    LuaTypeDecl, RenderLevel,
};
use emmylua_parser::VisibilityKind;
use std::path::Path;
use tera::{Context, Tera};

use crate::markdown_generator::{
    escape_type_name,
    gen::collect_property,
    markdown_types::{Doc, IndexStruct, MemberDoc, MkdocsIndex},
    render::{render_const_type, render_function_type},
};

pub fn generate_type_markdown(
    db: &DbIndex,
    tl: &Tera,
    typ: &LuaTypeDecl,
    output: &Path,
    mkdocs_index: &mut MkdocsIndex,
) -> Option<()> {
    check_filter(db, typ)?;
    let mut context = tera::Context::new();
    let typ_name = typ.get_name();
    let mut doc = Doc::default();
    doc.name = typ_name.to_string();

    if typ.is_class() {
        generate_class_type_markdown(db, tl, typ, &mut doc, &mut context, output, mkdocs_index);
    } else if typ.is_enum() {
        generate_enum_type_markdown(db, tl, typ, &mut doc, &mut context, output, mkdocs_index);
    } else {
        generate_alias_type_markdown(db, tl, typ, &mut doc, &mut context, output, mkdocs_index);
    }
    Some(())
}

fn check_filter(db: &DbIndex, typ: &LuaTypeDecl) -> Option<()> {
    let location = typ.get_locations();
    for loc in location {
        let file_id = loc.file_id;
        let module = db.get_module_index().get_module(file_id)?;
        if module.workspace_id.is_main() {
            return Some(());
        }
    }

    None
}

fn generate_class_type_markdown(
    db: &DbIndex,
    tl: &Tera,
    typ: &LuaTypeDecl,
    doc: &mut Doc,
    context: &mut Context,
    output: &Path,
    mkdocs_index: &mut MkdocsIndex,
) -> Option<()> {
    let typ_name = typ.get_name();
    let typ_id = typ.get_id();
    let namespace = typ.get_namespace();
    if let Some(namespace) = namespace {
        doc.namespace = Some(namespace.to_string());
    }

    let type_property_id = LuaSemanticDeclId::TypeDecl(typ_id.clone());
    doc.property = collect_property(db, type_property_id);

    let supers = db.get_type_index().get_super_types(&typ_id);
    if let Some(supers) = supers {
        let mut super_type_texts = Vec::new();
        for super_typ in supers {
            let super_type_text = humanize_type(db, &super_typ, RenderLevel::Simple);
            super_type_texts.push(super_type_text);
        }
        doc.supers = Some(super_type_texts.join(", "));
    }

    let member_owner = LuaMemberOwner::Type(typ_id);
    let members = db.get_member_index().get_sorted_members(&member_owner);
    let mut method_members: Vec<MemberDoc> = Vec::new();
    let mut field_members: Vec<MemberDoc> = Vec::new();
    if let Some(members) = members {
        for member in members {
            let member_typ = db
                .get_type_index()
                .get_type_cache(&member.get_id().into())
                .unwrap_or(&LuaTypeCache::InferType(LuaType::Unknown))
                .as_type();
            let member_id = member.get_id();
            let member_property_id = LuaSemanticDeclId::Member(member_id);
            let member_property = db.get_property_index().get_property(&member_property_id);
            if let Some(member_property) = member_property {
                if member_property.visibility.unwrap_or(VisibilityKind::Public)
                    != VisibilityKind::Public
                {
                    continue;
                }
            }

            let member_property = collect_property(db, member_property_id);

            let member_key = member.get_key();
            let name = match member_key {
                LuaMemberKey::Name(name) => name.to_string(),
                LuaMemberKey::Integer(i) => format!("[{}]", i),
                _ => continue,
            };

            let title_name = format!("{}.{}", typ_name, name);
            if member_typ.is_function() {
                let func_name = format!("{}.{}", typ_name, name);
                let display = render_function_type(db, member_typ, &func_name, false);
                method_members.push(MemberDoc {
                    name: title_name,
                    display,
                    property: member_property,
                });
            } else if member_typ.is_const() {
                let const_type_display = render_const_type(db, member_typ);
                field_members.push(MemberDoc {
                    name: title_name,
                    display: format!(
                        "```lua\n{}.{}: {}\n```\n",
                        typ_name, name, const_type_display
                    ),
                    property: member_property,
                });
            } else {
                let typ_display = humanize_type(db, member_typ, RenderLevel::Detailed);
                field_members.push(MemberDoc {
                    name: title_name,
                    display: format!("```lua\n{}.{} : {}\n```\n", typ_name, name, typ_display),
                    property: member_property,
                });
            }
        }
    }
    if !method_members.is_empty() {
        doc.methods = Some(method_members);
    }

    if !field_members.is_empty() {
        doc.fields = Some(field_members);
    }

    context.insert("doc", &doc);
    let render_text = match tl.render("lua_type_template.tl", context) {
        Ok(text) => text,
        Err(e) => {
            log::error!("Failed to render template: {}", e);
            return None;
        }
    };

    let file_type_name = format!("{}.md", escape_type_name(typ.get_full_name()));
    mkdocs_index.types.push(IndexStruct {
        name: format!("class {}", typ_name),
        file: format!("types/{}", file_type_name.clone()),
    });

    let outpath = output.join(file_type_name);
    log::info!("Writing class file: {}", outpath.display());
    match std::fs::write(outpath, render_text) {
        Ok(_) => {}
        Err(e) => {
            log::error!("Failed to write file: {}", e);
            return None;
        }
    }
    Some(())
}

fn generate_enum_type_markdown(
    db: &DbIndex,
    tl: &Tera,
    typ: &LuaTypeDecl,
    doc: &mut Doc,
    context: &mut Context,
    output: &Path,
    mkdocs_index: &mut MkdocsIndex,
) -> Option<()> {
    let typ_name = typ.get_name();
    let typ_id = typ.get_id();
    let namespace = typ.get_namespace();
    if let Some(namespace) = namespace {
        doc.namespace = Some(namespace.to_string());
    }
    doc.property = collect_property(db, LuaSemanticDeclId::TypeDecl(typ_id.clone()));

    let member_owner = LuaMemberOwner::Type(typ_id);
    let members = db.get_member_index().get_sorted_members(&member_owner);
    let mut field_members: Vec<MemberDoc> = Vec::new();
    if let Some(members) = members {
        for member in members {
            let member_typ = db
                .get_type_index()
                .get_type_cache(&member.get_id().into())
                .unwrap_or(&LuaTypeCache::InferType(LuaType::Unknown))
                .as_type();
            let member_id = member.get_id();
            let member_property_id = LuaSemanticDeclId::Member(member_id);
            let member_property = db.get_property_index().get_property(&member_property_id);
            if let Some(member_property) = member_property {
                if member_property.visibility.unwrap_or(VisibilityKind::Public)
                    != VisibilityKind::Public
                {
                    continue;
                }
            }

            let member_property = collect_property(db, member_property_id);

            let member_key = member.get_key();
            let name = match member_key {
                LuaMemberKey::Name(name) => name,
                _ => continue,
            };

            let typ_display = humanize_type(db, member_typ, RenderLevel::Simple);
            field_members.push(MemberDoc {
                name: name.to_string(),
                display: typ_display,
                property: member_property,
            });
        }
    }

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

    let file_type_name = format!("{}.md", escape_type_name(typ.get_full_name()));
    mkdocs_index.types.push(IndexStruct {
        name: format!("enum {}", typ_name),
        file: format!("types/{}", file_type_name.clone()),
    });

    let outpath = output.join(file_type_name);
    log::info!("Writing enum file: {}", outpath.display());
    match std::fs::write(outpath, render_text) {
        Ok(_) => {}
        Err(e) => {
            log::error!("Failed to write file: {}", e);
            return None;
        }
    }
    Some(())
}

fn generate_alias_type_markdown(
    db: &DbIndex,
    tl: &Tera,
    typ: &LuaTypeDecl,
    doc: &mut Doc,
    context: &mut Context,
    output: &Path,
    mkdocs_index: &mut MkdocsIndex,
) -> Option<()> {
    let typ_name = typ.get_name();
    let typ_id = typ.get_id();
    let namespace = typ.get_namespace();
    if let Some(namespace) = namespace {
        doc.namespace = Some(namespace.to_string());
    }

    let type_property_id = LuaSemanticDeclId::TypeDecl(typ_id.clone());
    doc.property = collect_property(db, type_property_id);

    if let Some(origin_typ) = typ.get_alias_origin(db, None) {
        let origin_type_display = humanize_type(db, &origin_typ, RenderLevel::Detailed);
        let display = format!(
            "```lua\n(alias) {} = {}\n```\n",
            typ_name, origin_type_display
        );
        doc.display = Some(display);
    }

    context.insert("doc", &doc);

    let render_text = match tl.render("lua_alias_template.tl", context) {
        Ok(text) => text,
        Err(e) => {
            log::error!("Failed to render template: {}", e);
            return None;
        }
    };

    let file_type_name = format!("{}.md", escape_type_name(typ.get_full_name()));
    mkdocs_index.types.push(IndexStruct {
        name: format!("alias {}", typ_name),
        file: format!("types/{}", file_type_name.clone()),
    });

    let outpath = output.join(file_type_name);
    log::info!("Writing alias file: {}", outpath.display());
    match std::fs::write(outpath, render_text) {
        Ok(_) => {}
        Err(e) => {
            log::error!("Failed to write file: {}", e);
            return None;
        }
    }
    Some(())
}

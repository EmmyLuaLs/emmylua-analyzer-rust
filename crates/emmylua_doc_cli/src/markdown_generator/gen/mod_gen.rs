use std::path::Path;

use emmylua_code_analysis::{
    humanize_type, DbIndex, FileId, LuaMemberKey, LuaMemberOwner, LuaSemanticDeclId, LuaType,
    LuaTypeCache, ModuleInfo, RenderLevel,
};
use emmylua_parser::VisibilityKind;
use tera::Tera;

use crate::markdown_generator::{
    escape_type_name,
    markdown_types::{Doc, IndexStruct, MemberDoc, MkdocsIndex},
    render::{render_const_type, render_function_type},
};

use super::collect_property;

pub fn generate_module_markdown(
    db: &DbIndex,
    tl: &Tera,
    module: &ModuleInfo,
    output: &Path,
    mkdocs_index: &mut MkdocsIndex,
) -> Option<()> {
    check_filter(db, module.file_id)?;

    let mut context = tera::Context::new();
    let mut doc = Doc::default();
    doc.name = module.full_module_name.clone();
    let property_owner_id = module.property_owner_id.clone();
    if let Some(property_id) = property_owner_id {
        doc.property = collect_property(db, property_id);
    }

    let export_typ = module.export_type.clone()?;
    match &export_typ {
        LuaType::Def(type_id) => {
            let member_owner = LuaMemberOwner::Type(type_id.clone());
            let type_simple_name = type_id.get_simple_name();
            generate_member_owner_module(db, member_owner, type_simple_name, &mut doc);
        }
        LuaType::TableConst(t) => {
            let member_owner = LuaMemberOwner::Element(t.clone());
            generate_member_owner_module(db, member_owner, "M", &mut doc);
        }
        LuaType::Instance(i) => {
            let member_owner = LuaMemberOwner::Element(i.get_range().clone());
            generate_member_owner_module(db, member_owner, "M", &mut doc);
        }
        _ => {}
    }

    context.insert("doc", &doc);

    let render_text = match tl.render("lua_module_template.tl", &context) {
        Ok(text) => text,
        Err(e) => {
            log::error!("Failed to render template: {}", e);
            return None;
        }
    };

    let file_name = format!("{}.md", escape_type_name(&module.full_module_name));
    mkdocs_index.modules.push(IndexStruct {
        name: module.full_module_name.clone(),
        file: format!("modules/{}", file_name.clone()),
    });

    let outpath = output.join(file_name);
    log::info!("Writing module file: {}", outpath.display());
    match std::fs::write(outpath, render_text) {
        Ok(_) => {}
        Err(e) => {
            log::error!("Failed to write file: {}", e);
            return None;
        }
    }
    Some(())
}

fn check_filter(db: &DbIndex, file_id: FileId) -> Option<()> {
    let module = db.get_module_index().get_module(file_id)?;
    if module.workspace_id.is_main() {
        return Some(());
    }

    None
}

pub fn generate_member_owner_module(
    db: &DbIndex,
    member_owner: LuaMemberOwner,
    owner_name: &str,
    doc: &mut Doc,
) -> Option<()> {
    let members = db.get_member_index().get_sorted_members(&member_owner);
    let mut method_members: Vec<MemberDoc> = Vec::new();
    let mut field_members: Vec<MemberDoc> = Vec::new();

    if let Some(members) = members {
        for member in members {
            let member_type = db
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

            let title_name = format!("{}.{}", owner_name, name);
            if member_type.is_function() {
                let func_name = format!("{}.{}", owner_name, name);
                let display = render_function_type(db, member_type, &func_name, false);
                method_members.push(MemberDoc {
                    name: title_name,
                    display,
                    property: member_property,
                });
            } else if member_type.is_const() {
                let display = render_const_type(db, member_type);
                field_members.push(MemberDoc {
                    name: title_name,
                    display: format!("```lua\n{}.{}: {}\n```\n", owner_name, name, display),
                    property: member_property,
                });
            } else {
                let typ_display = humanize_type(db, member_type, RenderLevel::Detailed);
                field_members.push(MemberDoc {
                    name: title_name,
                    display: format!("```lua\n{}.{} : {}\n```\n", owner_name, name, typ_display),
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

    Some(())
}

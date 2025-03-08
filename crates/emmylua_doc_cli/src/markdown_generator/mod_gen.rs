use std::path::Path;

use emmylua_code_analysis::{
    humanize_type, DbIndex, FileId, LuaMemberKey, LuaMemberOwner, LuaPropertyOwnerId, LuaType,
    ModuleInfo, RenderLevel,
};
use emmylua_parser::VisibilityKind;
use tera::{Context, Tera};

use crate::markdown_generator::{escape_type_name, IndexStruct, MemberDisplay};

use super::{
    render::{render_const_type, render_function_type},
    MkdocsIndex,
};

pub fn generate_module_markdown(
    db: &DbIndex,
    tl: &Tera,
    module: &ModuleInfo,
    output: &Path,
    mkdocs_index: &mut MkdocsIndex,
) -> Option<()> {
    check_filter(db, module.file_id)?;

    let mut context = tera::Context::new();
    context.insert("module_name", &module.full_module_name);

    let export_typ = module.export_type.clone()?;
    match &export_typ {
        LuaType::Def(type_id) => {
            let member_owner = LuaMemberOwner::Type(type_id.clone());
            let type_simple_name = type_id.get_simple_name();
            generate_member_owner_module(db, member_owner, type_simple_name, &mut context);
        }
        LuaType::TableConst(t) => {
            let member_owner = LuaMemberOwner::Element(t.clone());
            generate_member_owner_module(db, member_owner, "M", &mut context);
        }
        LuaType::Instance(i) => {
            let member_owner = LuaMemberOwner::Element(i.get_range().clone());
            generate_member_owner_module(db, member_owner, "M", &mut context);
        }
        _ => {}
    }

    let render_text = match tl.render("lua_module_template.tl", &context) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("Failed to render template: {}", e);
            return None;
        }
    };

    let file_name = format!("{}.md", escape_type_name(&module.full_module_name));
    mkdocs_index.modules.push(IndexStruct {
        name: module.full_module_name.clone(),
        file: format!("modules/{}", file_name.clone()),
    });

    let outpath = output.join(file_name);
    println!("output module file: {}", outpath.display());
    match std::fs::write(outpath, render_text) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Failed to write file: {}", e);
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
    context: &mut Context,
) -> Option<()> {
    let member_map = db.get_member_index().get_member_map(member_owner);
    let mut method_members: Vec<MemberDisplay> = Vec::new();
    let mut field_members: Vec<MemberDisplay> = Vec::new();

    if let Some(member_map) = member_map {
        let mut member_vecs = member_map.iter().map(|(k, v)| (k, v)).collect::<Vec<_>>();
        member_vecs.sort_by(|a, b| a.0.cmp(b.0));

        for (member_name, member_id) in member_vecs {
            let member = db.get_member_index().get_member(member_id)?;
            let member_typ = member.get_decl_type();
            let member_property_id = LuaPropertyOwnerId::Member(member_id.clone());
            let member_property = db.get_property_index().get_property(member_property_id);
            if let Some(member_property) = member_property {
                if member_property.visibility.unwrap_or(VisibilityKind::Public)
                    != VisibilityKind::Public
                {
                    continue;
                }
            }

            let description = if let Some(member_property) = member_property {
                *member_property
                    .description
                    .clone()
                    .unwrap_or("".to_string().into())
            } else {
                "".to_string()
            };

            let name = match member_name {
                LuaMemberKey::Name(name) => name,
                _ => continue,
            };

            let title_name = format!("{}.{}", owner_name, name);
            if member_typ.is_function() {
                let func_name = format!("{}.{}", owner_name, name);
                let display = render_function_type(db, &member_typ, &func_name, false);
                method_members.push(MemberDisplay {
                    name: title_name,
                    display,
                    description,
                });
            } else if member_typ.is_const() {
                let display = render_const_type(db, &member_typ);
                field_members.push(MemberDisplay {
                    name: title_name,
                    display: format!("```lua\n{}.{}: {}\n```\n", owner_name, name, display),
                    description,
                });
            } else {
                let typ_display = humanize_type(db, &member_typ, RenderLevel::Detailed);
                field_members.push(MemberDisplay {
                    name: title_name,
                    display: format!("```lua\n{}.{} : {}\n```\n", owner_name, name, typ_display),
                    description,
                });
            }
        }
    }
    if !method_members.is_empty() {
        context.insert("methods", &method_members);
    }

    if !field_members.is_empty() {
        context.insert("fields", &field_members);
    }

    Some(())
}

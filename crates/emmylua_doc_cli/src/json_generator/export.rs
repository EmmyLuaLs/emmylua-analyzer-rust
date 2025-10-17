use crate::common::{render_const, render_typ};
use crate::json_generator::json_types::*;
use emmylua_code_analysis::{
    AsyncState, DbIndex, FileId, LuaDeprecated, LuaMemberKey, LuaMemberOwner, LuaNoDiscard,
    LuaSemanticDeclId, LuaSignature, LuaType, LuaTypeCache, LuaTypeDecl, LuaTypeDeclId,
    RenderLevel, Vfs,
};
use emmylua_parser::VisibilityKind;
use rowan::TextRange;

pub fn export(db: &DbIndex) -> Index {
    Index {
        modules: export_modules(db),
        types: export_types(db),
        globals: export_globals(db),
        config: db.get_emmyrc().clone(),
    }
}

fn export_modules(db: &DbIndex) -> Vec<Module> {
    let type_index = db.get_type_index();
    let module_index = db.get_module_index();
    let modules = module_index.get_module_infos();
    let vfs = db.get_vfs();

    modules
        .into_iter()
        .filter(|module| module_index.is_main(&module.file_id))
        .filter_map(|module| {
            let (members, typ) = match module.export_type.as_ref()? {
                LuaType::TableConst(t) => {
                    let member_owner = LuaMemberOwner::Element(t.clone());
                    (export_members(db, member_owner), None)
                }
                LuaType::Instance(i) => {
                    let member_owner = LuaMemberOwner::Element(i.get_range().clone());
                    (export_members(db, member_owner), None)
                }
                typ => (Vec::new(), Some(render_typ(db, typ, RenderLevel::Simple))),
            };

            let property = module
                .semantic_id
                .as_ref()
                .map(|decl_id| export_property(db, decl_id))
                .unwrap_or_default();

            let namespace = type_index.get_file_namespace(&module.file_id).cloned();
            let using = type_index
                .get_file_using_namespace(&module.file_id)
                .cloned()
                .unwrap_or_default();

            Some(Module {
                name: module.full_module_name.clone(),
                property,
                file: vfs.get_file_path(&module.file_id).cloned(),
                typ,
                members,
                namespace,
                using,
            })
        })
        .collect()
}

fn export_types(db: &DbIndex) -> Vec<Type> {
    let type_index = db.get_type_index();
    let module_index = db.get_module_index();
    let types = type_index.get_all_types();

    types
        .into_iter()
        .filter(|type_decl| {
            type_decl
                .get_locations()
                .iter()
                .any(|loc| module_index.is_main(&loc.file_id))
        })
        .flat_map(|type_decl| {
            if type_decl.is_class() {
                Some(Type::Class(export_class(db, type_decl)))
            } else if type_decl.is_enum() {
                Some(Type::Enum(export_enum(db, type_decl)))
            } else if type_decl.is_alias() {
                Some(Type::Alias(export_alias(db, type_decl)))
            } else {
                None
            }
        })
        .collect()
}

fn export_globals(db: &DbIndex) -> Vec<Global> {
    let global_index = db.get_global_index();
    let module_index = db.get_module_index();
    let type_index = db.get_type_index();
    let vfs = db.get_vfs();
    let globals = global_index.get_all_global_decl_ids();

    globals
        .into_iter()
        .filter(|global| module_index.is_main(&global.file_id))
        .filter_map(|global| {
            let decl = db.get_decl_index().get_decl(&global)?;
            let typ = type_index.get_type_cache(&global.into())?.as_type();
            let property = export_property(db, &LuaSemanticDeclId::LuaDecl(global));
            let loc = export_loc(vfs, decl.get_file_id(), decl.get_range());
            match typ {
                LuaType::TableConst(table) => {
                    let member_owner = LuaMemberOwner::Element(table.clone());
                    Some(Global::Table(GlobalTable {
                        name: decl.get_name().to_string(),
                        property,
                        loc,
                        members: export_members(db, member_owner),
                    }))
                }
                _ => Some(Global::Field(GlobalField {
                    name: decl.get_name().to_string(),
                    property,
                    loc,
                    typ: render_typ(db, typ, RenderLevel::Simple),
                    literal: render_const(typ),
                })),
            }
        })
        .collect()
}

fn export_class(db: &DbIndex, type_decl: &LuaTypeDecl) -> Class {
    let type_decl_id = type_decl.get_id();
    let type_index = db.get_type_index();
    let property = export_property(db, &LuaSemanticDeclId::TypeDecl(type_decl.get_id().clone()));
    let member_owner = LuaMemberOwner::Type(type_decl_id.clone());

    Class {
        name: type_decl.get_full_name().to_string(),
        property,
        loc: export_loc_for_type(db, type_decl),
        bases: type_index
            .get_super_types(&type_decl_id)
            .unwrap_or_default()
            .iter()
            .map(|typ| render_typ(db, typ, RenderLevel::Simple))
            .collect(),
        generics: export_generics(db, &type_decl_id),
        members: export_members(db, member_owner),
    }
}

fn export_alias(db: &DbIndex, type_decl: &LuaTypeDecl) -> Alias {
    let type_decl_id = type_decl.get_id();
    let property = export_property(db, &LuaSemanticDeclId::TypeDecl(type_decl.get_id().clone()));
    let member_owner = LuaMemberOwner::Type(type_decl_id.clone());

    Alias {
        name: type_decl.get_full_name().to_string(),
        property,
        loc: export_loc_for_type(db, type_decl),
        typ: type_decl
            .get_alias_ref()
            .map(|typ| render_typ(db, typ, RenderLevel::Documentation)),
        generics: export_generics(db, &type_decl_id),
        members: export_members(db, member_owner),
    }
}

fn export_enum(db: &DbIndex, type_decl: &LuaTypeDecl) -> Enum {
    let type_decl_id = type_decl.get_id();
    let property = export_property(db, &LuaSemanticDeclId::TypeDecl(type_decl.get_id().clone()));
    let member_owner = LuaMemberOwner::Type(type_decl_id.clone());

    Enum {
        name: type_decl.get_full_name().to_string(),
        property,
        loc: export_loc_for_type(db, type_decl),
        typ: type_decl
            .get_enum_field_type(db)
            .map(|typ| render_typ(db, &typ, RenderLevel::Simple)),
        generics: export_generics(db, &type_decl_id),
        members: export_members(db, member_owner),
    }
}

fn export_generics(db: &DbIndex, type_decl_id: &LuaTypeDeclId) -> Vec<TypeVar> {
    let type_index = db.get_type_index();

    type_index
        .get_generic_params(type_decl_id)
        .map(|v| v.as_slice())
        .unwrap_or_default()
        .iter()
        .map(|it| TypeVar {
            name: it.name.to_string(),
            base: it
                .type_constraint
                .as_ref()
                .map(|typ| render_typ(db, typ, RenderLevel::Simple)),
        })
        .collect()
}

fn export_members(db: &DbIndex, member_owner: LuaMemberOwner) -> Vec<Member> {
    let member_index = db.get_member_index();
    let type_index = db.get_type_index();
    let vfs = db.get_vfs();
    let members = member_index.get_sorted_members(&member_owner);
    if let Some(members) = members {
        members
            .into_iter()
            .filter_map(|member| {
                let typ = type_index
                    .get_type_cache(&member.get_id().into())
                    .unwrap_or(&LuaTypeCache::InferType(LuaType::Unknown))
                    .as_type();

                let member_key = member.get_key();
                let name = match member_key {
                    LuaMemberKey::Name(name) => name.to_string(),
                    LuaMemberKey::Integer(i) => format!("[{i}]"),
                    LuaMemberKey::ExprType(typ) => {
                        format!("[{}]", render_typ(db, typ, RenderLevel::Simple))
                    }
                    _ => return None,
                };

                let member_id = member.get_id();
                let member_property_id = LuaSemanticDeclId::Member(member_id);
                let property = export_property(db, &member_property_id);

                let loc = export_loc(vfs, member.get_file_id(), member.get_range());

                match typ {
                    LuaType::Signature(signature_id) => {
                        db.get_signature_index().get(signature_id).map(|signature| {
                            Member::Fn(export_signature(db, signature, name, property, loc))
                        })
                    }
                    _ => Some(Member::Field(export_field(db, typ, name, property, loc))),
                }
            })
            .collect()
    } else {
        Default::default()
    }
}

fn export_signature(
    db: &DbIndex,
    signature: &LuaSignature,
    name: String,
    property: Property,
    loc: Option<Loc>,
) -> Fn {
    Fn {
        name,
        property,
        loc,
        generics: signature
            .generic_params
            .iter()
            .map(|generic_param| TypeVar {
                name: generic_param.name.to_string(),
                base: generic_param
                    .type_constraint
                    .as_ref()
                    .map(|typ| render_typ(db, typ, RenderLevel::Simple)),
            })
            .collect(),
        params: signature
            .params
            .iter()
            .enumerate()
            .map(|(i, name)| match signature.param_docs.get(&i) {
                Some(param_info) => FnParam {
                    name: Some(name.clone()),
                    typ: Some(render_typ(db, &param_info.type_ref, RenderLevel::Simple)),
                    desc: param_info.description.clone(),
                },
                None => FnParam {
                    name: Some(name.clone()),
                    ..Default::default()
                },
            })
            .collect(),
        returns: signature
            .return_docs
            .iter()
            .map(|ret| FnParam {
                name: ret.name.clone(),
                typ: Some(render_typ(db, &ret.type_ref, RenderLevel::Simple)),
                desc: ret.description.clone(),
            })
            .collect(),
        overloads: signature
            .overloads
            .iter()
            .map(|overload| {
                render_typ(
                    db,
                    &LuaType::DocFunction(overload.clone()),
                    RenderLevel::Simple,
                )
            })
            .collect(),
        is_async: signature.async_state == AsyncState::Async,
        is_meth: signature.is_colon_define,
        is_nodiscard: signature.nodiscard.is_some(),
        nodiscard_message: match &signature.nodiscard {
            Some(LuaNoDiscard::NoDiscardWithMessage(msg)) => Some(msg.to_string()),
            _ => None,
        },
    }
}

fn export_field(
    db: &DbIndex,
    typ: &LuaType,
    name: String,
    property: Property,
    loc: Option<Loc>,
) -> Field {
    Field {
        name,
        property,
        loc,
        typ: render_typ(db, typ, RenderLevel::Simple),
        literal: render_const(typ),
    }
}

fn export_property(db: &DbIndex, semantic_decl: &LuaSemanticDeclId) -> Property {
    match db.get_property_index().get_property(semantic_decl) {
        Some(property) => Property {
            description: property.description().map(|s| s.to_string()),
            visibility: if property.visibility == VisibilityKind::Public {
                None
            } else {
                Some(property.visibility.to_str().unwrap_or_default().to_string())
            },
            deprecated: property.deprecated().is_some(),
            deprecation_reason: property.deprecated().as_ref().and_then(|s| match s {
                LuaDeprecated::Deprecated => None,
                LuaDeprecated::DeprecatedWithMessage(msg) => Some(msg.to_string()),
            }),
            tag_content: property.tag_content().map(|tag_content| {
                tag_content
                    .get_all_tags()
                    .iter()
                    .map(|(name, content)| TagNameContent {
                        tag_name: name.clone(),
                        content: content.clone(),
                    })
                    .collect()
            }),
        },
        None => Default::default(),
    }
}

fn export_loc_for_type(db: &DbIndex, type_decl: &LuaTypeDecl) -> Vec<Loc> {
    let vfs = db.get_vfs();
    type_decl
        .get_locations()
        .iter()
        .filter_map(|loc| export_loc(vfs, loc.file_id, loc.range))
        .collect()
}

fn export_loc(vfs: &Vfs, file_id: FileId, range: TextRange) -> Option<Loc> {
    vfs.get_document(&file_id).map(|document| Loc {
        file: document.get_file_path().clone(),
        line: document.get_line(range.start()).unwrap_or_default() + 1,
    })
}

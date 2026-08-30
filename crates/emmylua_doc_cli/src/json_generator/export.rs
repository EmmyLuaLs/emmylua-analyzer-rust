use std::sync::Arc;

use crate::common::{render_const, render_typ};
use crate::doc_model::{DocFunctionInfo, DocModel, DocProperty, DocType};
use crate::json_generator::json_types::*;
use emmylua_code_analysis::{Emmyrc, LuaType, RenderLevel};
use emmylua_parser::VisibilityKind;

pub fn export(model: &DocModel, config: Arc<Emmyrc>) -> Index {
    Index {
        modules: export_modules(model),
        types: export_types(model),
        globals: export_globals(model),
        config: config.as_ref().clone(),
    }
}

fn export_modules(model: &DocModel) -> Vec<Module> {
    model
        .modules
        .iter()
        .map(|module| {
            let (members, typ) = match &module.export_type {
                LuaType::TableConst(_) | LuaType::Instance(_) => (
                    export_members(model, &model.members_of_type(&module.export_type)),
                    None,
                ),
                typ => (
                    Vec::new(),
                    Some(render_typ(model, typ, RenderLevel::Simple)),
                ),
            };

            Module {
                name: module.name.clone(),
                property: property_json(&module.property),
                file: module.path.clone(),
                typ,
                members,
                namespace: module.namespace.clone(),
                using: module.usings.clone(),
            }
        })
        .collect()
}

fn export_types(model: &DocModel) -> Vec<Type> {
    model
        .types
        .iter()
        .map(|doc_type| match doc_type.kind {
            crate::doc_model::DocTypeKind::Class => Type::Class(export_class(model, doc_type)),
            crate::doc_model::DocTypeKind::Enum => Type::Enum(export_enum(model, doc_type)),
            crate::doc_model::DocTypeKind::Alias => Type::Alias(export_alias(model, doc_type)),
        })
        .collect()
}

fn export_globals(model: &DocModel) -> Vec<Global> {
    model
        .globals
        .iter()
        .map(|global| {
            let property = property_json(&global.property);
            let loc = global.loc.as_ref().map(loc_json);
            match &global.ty {
                LuaType::TableConst(_) | LuaType::Instance(_) => Global::Table(GlobalTable {
                    name: global.name.clone(),
                    property,
                    loc,
                    members: export_members(model, &model.members_of_type(&global.ty)),
                }),
                typ => Global::Field(GlobalField {
                    name: global.name.clone(),
                    property,
                    loc,
                    typ: render_typ(model, typ, RenderLevel::Simple),
                    literal: render_const(typ),
                }),
            }
        })
        .collect()
}

fn export_class(model: &DocModel, doc_type: &DocType) -> Class {
    Class {
        name: doc_type.full_name.clone(),
        property: property_json(&doc_type.property),
        loc: export_locs(doc_type),
        bases: doc_type
            .bases
            .iter()
            .map(|typ| render_typ(model, typ, RenderLevel::Simple))
            .collect(),
        generics: export_generics(model, &doc_type.generics),
        members: export_members(model, &doc_type.members),
    }
}

fn export_alias(model: &DocModel, doc_type: &DocType) -> Alias {
    Alias {
        name: doc_type.full_name.clone(),
        property: property_json(&doc_type.property),
        loc: export_locs(doc_type),
        typ: doc_type
            .alias_type
            .as_ref()
            .map(|typ| render_typ(model, typ, RenderLevel::Documentation)),
        generics: export_generics(model, &doc_type.generics),
        members: export_members(model, &doc_type.members),
    }
}

fn export_enum(model: &DocModel, doc_type: &DocType) -> Enum {
    let enum_typ = if doc_type.members.is_empty() {
        None
    } else {
        let fields = doc_type
            .members
            .iter()
            .map(|member| model.render_type(&member.ty, RenderLevel::Simple))
            .collect::<Vec<_>>()
            .join(" | ");
        Some(fields)
    };

    Enum {
        name: doc_type.full_name.clone(),
        property: property_json(&doc_type.property),
        loc: export_locs(doc_type),
        typ: enum_typ,
        generics: export_generics(model, &doc_type.generics),
        members: export_members(model, &doc_type.members),
    }
}

fn export_generics(model: &DocModel, generics: &[crate::doc_model::DocGeneric]) -> Vec<TypeVar> {
    generics
        .iter()
        .map(|generic| TypeVar {
            name: generic.name.clone(),
            base: generic
                .constraint
                .as_ref()
                .map(|typ| render_typ(model, typ, RenderLevel::Simple)),
        })
        .collect()
}

fn export_members(model: &DocModel, members: &[crate::doc_model::DocMember]) -> Vec<Member> {
    members
        .iter()
        .map(|member| {
            let property = property_json(&member.property);
            let loc = member.loc.as_ref().map(loc_json);
            if let Some(func) = model.function_info(&member.ty, member.signature.as_ref()) {
                Member::Fn(export_function(model, &member.name, property, loc, func))
            } else {
                Member::Field(export_field(
                    model,
                    &member.ty,
                    member.name.clone(),
                    property,
                    loc,
                ))
            }
        })
        .collect()
}

fn export_function(
    model: &DocModel,
    name: &str,
    property: Property,
    loc: Option<Loc>,
    func: DocFunctionInfo,
) -> Fn {
    Fn {
        name: name.to_string(),
        property,
        loc,
        generics: export_generics(model, &func.generics),
        params: func
            .params
            .iter()
            .map(|param| FnParam {
                name: Some(param.name.clone()),
                typ: param
                    .ty
                    .as_ref()
                    .map(|ty| render_typ(model, ty, RenderLevel::Simple)),
                desc: None,
            })
            .collect(),
        returns: func
            .returns
            .iter()
            .map(|ty| FnParam {
                name: None,
                typ: Some(render_typ(model, ty, RenderLevel::Simple)),
                desc: None,
            })
            .collect(),
        overloads: func
            .overloads
            .iter()
            .map(|ty| render_typ(model, ty, RenderLevel::Simple))
            .collect(),
        is_async: func.is_async,
        is_meth: func.is_method,
        is_nodiscard: func.is_nodiscard,
        nodiscard_message: func.nodiscard_message,
    }
}

fn export_field(
    model: &DocModel,
    typ: &LuaType,
    name: String,
    property: Property,
    loc: Option<Loc>,
) -> Field {
    Field {
        name,
        property,
        loc,
        typ: render_typ(model, typ, RenderLevel::Simple),
        literal: render_const(typ),
    }
}

fn property_json(property: &DocProperty) -> Property {
    Property {
        description: None,
        visibility: match property.visibility {
            Some(VisibilityKind::Public) | None => None,
            Some(visibility) => Some(visibility.to_str().unwrap_or_default().to_string()),
        },
        deprecated: property.deprecated,
        deprecation_reason: None,
        tag_content: None,
    }
}

fn export_locs(doc_type: &DocType) -> Vec<Loc> {
    doc_type.locations.iter().map(loc_json).collect()
}

fn loc_json(loc: &crate::doc_model::DocLoc) -> Loc {
    Loc {
        file: loc.file.clone(),
        line: loc.line,
    }
}

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaDocTag, LuaDocTagAlias, LuaDocTagClass, LuaDocTagEnum,
};

use crate::{DiagnosticCode, LuaTypeFlag, SemanticModel};

use super::{Checker, DiagnosticContext};

pub struct DuplicateTypeChecker;

impl Checker for DuplicateTypeChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::DuplicateType];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for tag in root.descendants::<LuaDocTag>() {
            match tag {
                LuaDocTag::Class(class_tag) => {
                    check_duplicate_class(context, class_tag);
                }
                LuaDocTag::Enum(enum_tag) => {
                    check_duplicate_enum(context, enum_tag);
                }
                LuaDocTag::Alias(alias_tag) => {
                    check_duplicate_alias(context, alias_tag);
                }
                _ => {}
            }
        }
    }
}

fn check_duplicate_class(context: &mut DiagnosticContext, class_tag: LuaDocTagClass) -> Option<()> {
    let file_id = context.file_id;
    let name_token = class_tag.get_name_token()?;
    let name = name_token.get_name_text();
    let range = name_token.get_range();
    let type_decl = context
        .get_db()
        .get_type_index()
        .find_type_decl(file_id, name)?;
    let locations = type_decl.get_locations();
    if locations.len() > 1 {
        let mut type_times = 0;
        let mut partial_times = 0;
        let mut constructor_times = 0;
        for location in locations {
            let flag = location.flag;
            if flag.contains(LuaTypeFlag::Meta) {
                continue;
            }
            if flag.contains(LuaTypeFlag::Partial) {
                partial_times += 1;
            } else if flag.contains(LuaTypeFlag::Constructor) {
                constructor_times += 1;
            } else {
                type_times += 1;
            }
        }

        if type_times > 1 && partial_times == 0 {
            context.add_diagnostic(
                DiagnosticCode::DuplicateType,
                range,
                t!("Duplicate class '%{name}', if this is intentional, please add the 'partial' attribute for every class define", name = name).to_string(),
                None,
            );
        } else if type_times > 0 && partial_times > 0 {
            context.add_diagnostic(
                DiagnosticCode::DuplicateType,
                range,
                t!("Duplicate class '%{name}'. The class %{name} is defined as both partial and non-partial.", name = name).to_string(),
                None,
            );
        }
        if constructor_times > 1 {
            context.add_diagnostic(
                DiagnosticCode::DuplicateType,
                range,
                t!(
                    "Duplicate class constructor '%{name}'. constructor must have only one.",
                    name = name
                )
                .to_string(),
                None,
            );
        }
    }

    Some(())
}

fn check_duplicate_enum(context: &mut DiagnosticContext, enum_tag: LuaDocTagEnum) -> Option<()> {
    let file_id = context.file_id;
    let name_token = enum_tag.get_name_token()?;
    let name = name_token.get_name_text();
    let range = name_token.get_range();
    let type_decl = context
        .get_db()
        .get_type_index()
        .find_type_decl(file_id, name)?;
    let locations = type_decl.get_locations();
    if locations.len() > 1 {
        let mut type_times = 0;
        let mut partial_times = 0;
        for location in locations {
            let flag = location.flag;
            if flag.contains(LuaTypeFlag::Meta) {
                continue;
            }
            if flag.contains(LuaTypeFlag::Partial) {
                partial_times += 1;
            } else {
                type_times += 1;
            }
        }

        if type_times > 1 && partial_times == 0 {
            context.add_diagnostic(
                DiagnosticCode::DuplicateType,
                range,
                t!("Duplicate enum '%{name}', if this is intentional, please add the 'partial' attribute for every enum define", name = name).to_string(),
                None,
            );
        } else if type_times > 0 && partial_times > 0 {
            context.add_diagnostic(
                DiagnosticCode::DuplicateType,
                range,
                t!("Duplicate enum '%{name}'. The enum %{name} is defined as both partial and non-partial.", name = name).to_string(),
                None,
            );
        }
    }

    Some(())
}

fn check_duplicate_alias(context: &mut DiagnosticContext, alias_tag: LuaDocTagAlias) -> Option<()> {
    let file_id = context.file_id;
    let name_token = alias_tag.get_name_token()?;
    let name = name_token.get_name_text();
    let range = name_token.get_range();
    let type_decl = context
        .get_db()
        .get_type_index()
        .find_type_decl(file_id, name)?;
    let locations = type_decl.get_locations();
    if locations.len() > 1 {
        let mut type_times = 0;
        for location in locations {
            let flag = location.flag;
            if !flag.contains(LuaTypeFlag::Meta) {
                type_times += 1;
            }
        }

        if type_times > 1 {
            context.add_diagnostic(
                DiagnosticCode::DuplicateType,
                range,
                t!(
                    "Duplicate alias '{name}'. Alias definitions cannot be partial.",
                    name = name
                )
                .to_string(),
                None,
            );
        }
    }

    Some(())
}

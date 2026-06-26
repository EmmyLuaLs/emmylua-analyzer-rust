//! Duplicate type checker — pure salsa.

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaDocTag, LuaDocTagAlias, LuaDocTagClass, LuaDocTagEnum,
};

use crate::DiagnosticCode;
use crate::compilation::SalsaDocTypeDefKindSummary;
use crate::semantic_model::SemanticModel;

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for tag in root.descendants::<LuaDocTag>() {
        match tag {
            LuaDocTag::Class(c) => check_class(context, model, c),
            LuaDocTag::Enum(e) => check_enum(context, model, e),
            LuaDocTag::Alias(a) => check_alias(context, model, a),
            _ => {}
        }
    }
}

fn check_class(context: &mut DiagnosticContext, model: &SemanticModel, tag: LuaDocTagClass) {
    let Some(name_tk) = tag.get_name_token() else {
        return;
    };
    let name = name_tk.get_name_text();
    let type_count = model.count_type_def_files(&name);
    if type_count > 1 {
        let Some(def) = model.get_type_def(&name) else {
            return;
        };
        let _is_partial = false; // TODO: check partial via doc tag properties
        if matches!(def.kind, SalsaDocTypeDefKindSummary::Class) {
            context.add_diagnostic(DiagnosticCode::DuplicateType, name_tk.get_range(),
                t!("Duplicate class '%{name}', if this is intentional, please add the 'partial' attribute", name = name).to_string(), None);
        }
    }
}

fn check_enum(context: &mut DiagnosticContext, model: &SemanticModel, tag: LuaDocTagEnum) {
    let Some(name_tk) = tag.get_name_token() else {
        return;
    };
    let name = name_tk.get_name_text();
    if model.count_type_def_files(&name) > 1 {
        context.add_diagnostic(
            DiagnosticCode::DuplicateType,
            name_tk.get_range(),
            t!("Duplicate enum '%{name}'", name = name).to_string(),
            None,
        );
    }
}

fn check_alias(context: &mut DiagnosticContext, model: &SemanticModel, tag: LuaDocTagAlias) {
    let Some(name_tk) = tag.get_name_token() else {
        return;
    };
    let name = name_tk.get_name_text();
    if model.count_type_def_files(&name) > 1 {
        context.add_diagnostic(
            DiagnosticCode::DuplicateType,
            name_tk.get_range(),
            t!("Duplicate alias '%{name}'", name = name).to_string(),
            None,
        );
    }
}

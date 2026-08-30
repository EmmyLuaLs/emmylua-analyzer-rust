//! duplicate_type: duplicate definitions of `@class`/`@alias`/`@enum` with the same name in the same scope.
//!
//! Mirrors the old `diagnostic::checker::duplicate_type`: counts by flag (partial/constructor/meta);
//! Definitions are aggregated through `type_def_locations` (scope-aware cross-file buckets).

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaDocTag, LuaDocTagAlias, LuaDocTagClass, LuaDocTagEnum,
};
use rowan::TextRange;

use crate::DiagnosticCode;
use crate::salsa_builder::def::TypeDefFlags;
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct DuplicateTypeChecker;

impl Checker for DuplicateTypeChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::DuplicateType];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for tag in root.descendants().filter_map(LuaDocTag::cast) {
            match tag {
                LuaDocTag::Class(class_tag) => {
                    check_duplicate_class(context, semantic_model, &class_tag);
                }
                LuaDocTag::Enum(enum_tag) => {
                    check_duplicate_enum(context, semantic_model, &enum_tag);
                }
                LuaDocTag::Alias(alias_tag) => {
                    check_duplicate_alias(context, semantic_model, &alias_tag);
                }
                _ => {}
            }
        }
    }
}

/// (type_times, partial_times, constructor_times): meta definitions are skipped.
fn count_flags(locations: &[crate::salsa_builder::def::TypeDef]) -> (usize, usize, usize) {
    let (mut type_times, mut partial_times, mut constructor_times) = (0, 0, 0);
    for location in locations {
        let TypeDefFlags {
            partial,
            constructor,
            meta,
        } = location.flags;
        if meta {
            continue;
        }
        if partial {
            partial_times += 1;
        } else if constructor {
            constructor_times += 1;
        } else {
            type_times += 1;
        }
    }
    (type_times, partial_times, constructor_times)
}

fn check_duplicate_class(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    class_tag: &LuaDocTagClass,
) {
    let Some(name_token) = class_tag.get_name_token() else {
        return;
    };
    let name = name_token.get_name_text();
    let range = name_token.get_range();
    let locations = semantic_model.type_def_locations(&name);
    if locations.len() <= 1 {
        return;
    }
    let (type_times, partial_times, constructor_times) = count_flags(&locations);
    if type_times > 1 && partial_times == 0 {
        report_duplicate(
            context,
            range,
            t!(
                "Duplicate class '%{name}', if this is intentional, please add the 'partial' attribute for every class define",
                name = name
            ),
        );
    } else if type_times > 0 && partial_times > 0 {
        report_duplicate(
            context,
            range,
            t!(
                "Duplicate class '%{name}'. The class %{name} is defined as both partial and non-partial.",
                name = name
            ),
        );
    }
    if constructor_times > 1 {
        report_duplicate(
            context,
            range,
            t!(
                "Duplicate class constructor '%{name}'. constructor must have only one.",
                name = name
            ),
        );
    }
}

fn check_duplicate_enum(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    enum_tag: &LuaDocTagEnum,
) {
    let Some(name_token) = enum_tag.get_name_token() else {
        return;
    };
    let name = name_token.get_name_text();
    let range = name_token.get_range();
    let locations = semantic_model.type_def_locations(&name);
    if locations.len() <= 1 {
        return;
    }
    let (type_times, partial_times, _) = count_flags(&locations);
    if type_times > 1 && partial_times == 0 {
        report_duplicate(
            context,
            range,
            t!(
                "Duplicate enum '%{name}', if this is intentional, please add the 'partial' attribute for every enum define",
                name = name
            ),
        );
    } else if type_times > 0 && partial_times > 0 {
        report_duplicate(
            context,
            range,
            t!(
                "Duplicate enum '%{name}'. The enum %{name} is defined as both partial and non-partial.",
                name = name
            ),
        );
    }
}

fn check_duplicate_alias(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    alias_tag: &LuaDocTagAlias,
) {
    let Some(name_token) = alias_tag.get_name_token() else {
        return;
    };
    let name = name_token.get_name_text();
    let range = name_token.get_range();
    let locations = semantic_model.type_def_locations(&name);
    if locations.len() <= 1 {
        return;
    }
    let (type_times, _, _) = count_flags(&locations);
    if type_times > 1 {
        report_duplicate(
            context,
            range,
            t!(
                "Duplicate alias '%{name}'. Alias definitions cannot be partial.",
                name = name
            ),
        );
    }
}

fn report_duplicate<T: AsRef<str>>(context: &mut CheckContext<'_>, range: TextRange, message: T) {
    context.add_diagnostic(DiagnosticCode::DuplicateType, range, message);
}

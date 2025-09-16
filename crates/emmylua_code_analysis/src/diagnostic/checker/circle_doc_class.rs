use std::collections::HashSet;

use emmylua_parser::{LuaAstNode, LuaAstToken, LuaDocTagClass};
use rowan::TextRange;

use crate::{DiagnosticCode, LuaType, SemanticModel};

use super::{Checker, DiagnosticContext};

pub struct CircleDocClassChecker;

impl Checker for CircleDocClassChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::CircleDocClass];

    /// 检查循环继承的类
    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();

        for expr in root.descendants::<LuaDocTagClass>() {
            check_doc_tag_class(context, semantic_model, &expr);
        }
    }
}

fn check_doc_tag_class(
    context: &mut DiagnosticContext,
    _: &SemanticModel,
    tag: &LuaDocTagClass,
) -> Option<()> {
    let type_index = context.db.get_type_index();

    let class_decl =
        type_index.find_type_decl(context.file_id, tag.get_name_token()?.get_name_text())?;

    if !class_decl.is_class() {
        return Some(());
    }

    let name = class_decl.get_full_name();

    let mut queue = Vec::new();
    let mut visited = HashSet::new();

    queue.push(class_decl.get_id());
    while let Some(current_id) = queue.pop() {
        if !visited.insert(current_id.clone()) {
            continue;
        }

        let super_types = type_index.get_super_types(&current_id);
        if let Some(super_types) = super_types {
            for super_type in super_types {
                if let LuaType::Ref(super_type_id) = &super_type {
                    if super_type_id.get_name() == name {
                        context.add_diagnostic(
                            DiagnosticCode::CircleDocClass,
                            get_lint_range(tag).unwrap_or(tag.get_range()),
                            t!("Circularly inherited classes.").to_string(),
                            None,
                        );
                        return Some(());
                    }

                    if !visited.contains(super_type_id) {
                        queue.push(super_type_id.clone());
                    }
                }
            }
        }
    }
    Some(())
}

fn get_lint_range(tag: &LuaDocTagClass) -> Option<TextRange> {
    let start = tag.get_name_token()?.get_range().start();
    let end = tag.get_supers()?.get_range().end();
    Some(TextRange::new(start, end))
}

use std::collections::HashSet;

use super::{EmmyAnnotator, EmmyAnnotatorType};
use crate::util::parse_desc;
use emmylua_code_analysis::{DocumentView, Emmyrc, SalsaSemanticModel, WorkspaceId};
use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaAstToken, LuaDocDescription, LuaForRangeStat, LuaForStat,
    LuaLocalFuncStat, LuaLocalStat, LuaNameExpr, LuaParamList, LuaVarExpr,
};
use emmylua_parser_desc::DescItemKind;
use rowan::TextRange;

/// Salsa-based annotator: aggregates declarations and references, and switches between read-only/mutable based on whether the variable is written.
pub fn build_annotators(
    model: &SalsaSemanticModel<'_>,
    document: &DocumentView,
    emmyrc: &Emmyrc,
) -> Vec<EmmyAnnotator> {
    let mut result = vec![];
    let mut use_range_set = HashSet::new();
    let Some(root) = model.chunk() else {
        return result;
    };

    for node in root.descendants::<LuaAst>() {
        match node {
            LuaAst::LuaLocalStat(local_stat) => {
                build_local_stat_annotator(
                    model,
                    document,
                    &mut use_range_set,
                    &mut result,
                    local_stat,
                );
            }
            LuaAst::LuaParamList(params_list) => {
                build_params_annotator(
                    model,
                    document,
                    &mut use_range_set,
                    &mut result,
                    params_list,
                );
            }
            LuaAst::LuaForStat(for_stat) => {
                build_for_stat_annotator(
                    model,
                    document,
                    &mut use_range_set,
                    &mut result,
                    for_stat,
                );
            }
            LuaAst::LuaForRangeStat(for_range_stat) => {
                build_for_range_annotator(
                    model,
                    document,
                    &mut use_range_set,
                    &mut result,
                    for_range_stat,
                );
            }
            LuaAst::LuaLocalFuncStat(local_func_stat) => {
                build_local_func_stat_annotator(
                    model,
                    document,
                    &mut use_range_set,
                    &mut result,
                    local_func_stat,
                );
            }
            LuaAst::LuaNameExpr(name_expr) => {
                build_name_expr_annotator(document, &mut use_range_set, &mut result, name_expr);
            }
            LuaAst::LuaDocDescription(description) => {
                if emmyrc.semantic_tokens.render_documentation_markup {
                    build_description_annotator(
                        model,
                        document,
                        emmyrc,
                        &mut use_range_set,
                        &mut result,
                        description,
                    );
                }
            }
            _ => {}
        }
    }

    result
}

fn build_local_stat_annotator(
    model: &SalsaSemanticModel<'_>,
    document: &DocumentView,
    use_range_set: &mut HashSet<TextRange>,
    result: &mut Vec<EmmyAnnotator>,
    local_stat: LuaLocalStat,
) -> Option<()> {
    for local_name in local_stat.get_local_name_list() {
        let name_token = local_name.get_name_token()?;
        let range = name_token.get_range();
        let decl = model.decl_by_offset(name_token.get_position())?;
        push_decl_annotator(
            model,
            document,
            use_range_set,
            result,
            decl,
            range,
            EmmyAnnotatorType::ReadOnlyLocal,
            EmmyAnnotatorType::MutLocal,
        );
    }
    Some(())
}

fn build_params_annotator(
    model: &SalsaSemanticModel<'_>,
    document: &DocumentView,
    use_range_set: &mut HashSet<TextRange>,
    result: &mut Vec<EmmyAnnotator>,
    param_list: LuaParamList,
) -> Option<()> {
    for param_name in param_list.get_params() {
        let name_token = param_name.get_name_token()?;
        let range = name_token.get_range();
        let decl = model.decl_by_offset(name_token.get_position())?;
        push_decl_annotator(
            model,
            document,
            use_range_set,
            result,
            decl,
            range,
            EmmyAnnotatorType::ReadonlyParam,
            EmmyAnnotatorType::MutParam,
        );
    }
    Some(())
}

fn build_for_stat_annotator(
    model: &SalsaSemanticModel<'_>,
    document: &DocumentView,
    use_range_set: &mut HashSet<TextRange>,
    result: &mut Vec<EmmyAnnotator>,
    for_stat: LuaForStat,
) -> Option<()> {
    let name_token = for_stat.get_var_name()?;
    let range = name_token.get_range();
    let decl = model.decl_by_offset(name_token.get_position())?;
    push_decl_annotator(
        model,
        document,
        use_range_set,
        result,
        decl,
        range,
        EmmyAnnotatorType::ReadonlyParam,
        EmmyAnnotatorType::MutParam,
    );
    Some(())
}

fn build_for_range_annotator(
    model: &SalsaSemanticModel<'_>,
    document: &DocumentView,
    use_range_set: &mut HashSet<TextRange>,
    result: &mut Vec<EmmyAnnotator>,
    for_stat: LuaForRangeStat,
) -> Option<()> {
    for name_token in for_stat.get_var_name_list() {
        let range = name_token.get_range();
        let decl = model.decl_by_offset(name_token.get_position())?;
        push_decl_annotator(
            model,
            document,
            use_range_set,
            result,
            decl,
            range,
            EmmyAnnotatorType::ReadonlyParam,
            EmmyAnnotatorType::MutParam,
        );
    }
    Some(())
}

fn build_local_func_stat_annotator(
    model: &SalsaSemanticModel<'_>,
    document: &DocumentView,
    use_range_set: &mut HashSet<TextRange>,
    result: &mut Vec<EmmyAnnotator>,
    local_func_stat: LuaLocalFuncStat,
) -> Option<()> {
    let local_name = local_func_stat.get_local_name()?;
    let name_token = local_name.get_name_token()?;
    let range = name_token.get_range();
    let decl = model.decl_by_offset(name_token.get_position())?;
    push_decl_annotator(
        model,
        document,
        use_range_set,
        result,
        decl,
        range,
        EmmyAnnotatorType::ReadOnlyLocal,
        EmmyAnnotatorType::MutLocal,
    );
    Some(())
}

fn build_name_expr_annotator(
    document: &DocumentView,
    use_range_set: &mut HashSet<TextRange>,
    result: &mut Vec<EmmyAnnotator>,
    name_expr: LuaNameExpr,
) -> Option<()> {
    let name_range = name_expr.get_range();
    if use_range_set.contains(&name_range) {
        return Some(());
    }

    let name_text = name_expr.get_name_text()?;
    if name_text == "self" || name_text == "_" {
        return Some(());
    }

    let mut annotator = EmmyAnnotator {
        typ: EmmyAnnotatorType::Global,
        ranges: vec![],
    };
    annotator.ranges.push(document.to_lsp_range(name_range)?);
    result.push(annotator);
    Some(())
}

fn build_description_annotator(
    _model: &SalsaSemanticModel<'_>,
    document: &DocumentView,
    emmyrc: &Emmyrc,
    use_range_set: &mut HashSet<TextRange>,
    result: &mut Vec<EmmyAnnotator>,
    description: LuaDocDescription,
) -> Option<()> {
    let text = document.get_text();
    let items = parse_desc(WorkspaceId::MAIN, emmyrc, text, description, None);

    let mut strong = EmmyAnnotator {
        typ: EmmyAnnotatorType::DocStrong,
        ranges: vec![],
    };
    let mut em = EmmyAnnotator {
        typ: EmmyAnnotatorType::DocEm,
        ranges: vec![],
    };

    for item in items {
        match item.kind {
            DescItemKind::Em => {
                use_range_set.insert(item.range);
                em.ranges.push(document.to_lsp_range(item.range)?);
            }
            DescItemKind::Strong => {
                use_range_set.insert(item.range);
                strong.ranges.push(document.to_lsp_range(item.range)?);
            }
            _ => {}
        }
    }

    result.push(em);
    result.push(strong);
    Some(())
}

#[allow(clippy::too_many_arguments)]
fn push_decl_annotator(
    model: &SalsaSemanticModel<'_>,
    document: &DocumentView,
    use_range_set: &mut HashSet<TextRange>,
    result: &mut Vec<EmmyAnnotator>,
    decl: emmylua_code_analysis::SemanticId,
    decl_range: TextRange,
    readonly_type: EmmyAnnotatorType,
    mut_type: EmmyAnnotatorType,
) -> Option<()> {
    let mut annotator = EmmyAnnotator {
        typ: readonly_type,
        ranges: vec![],
    };

    use_range_set.insert(decl_range);
    annotator.ranges.push(document.to_lsp_range(decl_range)?);

    let mut is_mut = false;
    for ref_syntax in model.decl_references(&decl) {
        let range = ref_syntax.get_range();
        use_range_set.insert(range);
        if is_write_reference(model, ref_syntax) {
            is_mut = true;
        }
        annotator.ranges.push(document.to_lsp_range(range)?);
    }

    if is_mut {
        annotator.typ = mut_type;
    }

    result.push(annotator);
    Some(())
}

fn is_write_reference(model: &SalsaSemanticModel<'_>, syntax: emmylua_parser::LuaSyntaxId) -> bool {
    let Some(tree) = model.syntax_tree() else {
        return false;
    };
    let root = tree.get_red_root();
    let Some(node) = syntax.to_node_from_root(&root) else {
        return false;
    };
    let Some(name_expr) = LuaNameExpr::cast(node) else {
        return false;
    };
    let Some(parent) = name_expr.syntax().parent() else {
        return false;
    };
    let Some(assign) = LuaAssignStat::cast(parent) else {
        return false;
    };
    let (vars, _) = assign.get_var_and_expr_list();
    vars.iter().any(|var| {
        matches!(
            var,
            LuaVarExpr::NameExpr(target) if target.get_range() == name_expr.get_range()
        )
    })
}

use std::collections::{HashMap, HashSet};

use emmylua_code_analysis::{
    DeclReferenceCell, FileId, LuaCompilation, LuaDeclId, LuaMemberId, LuaMemberKey,
    LuaSemanticDeclId, LuaType, LuaTypeDeclId, SemanticDeclLevel, SemanticModel,
};
use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaAstToken, LuaCallExpr, LuaNameToken, LuaStringToken,
    LuaSyntaxNode, LuaSyntaxToken, LuaTableField,
};
use lsp_types::Location;

#[derive(Default)]
struct ReferenceSearchContext {
    visited_module_exports: HashSet<FileId>,
}

pub fn search_references(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    token: LuaSyntaxToken,
) -> Option<Vec<Location>> {
    let mut result = Vec::new();
    if let Some(semantic_decl) =
        semantic_model.find_decl(token.clone().into(), SemanticDeclLevel::default())
    {
        match semantic_decl {
            LuaSemanticDeclId::LuaDecl(decl_id) => {
                search_decl_references(semantic_model, compilation, decl_id, &mut result);
            }
            LuaSemanticDeclId::Member(member_id) => {
                search_member_references(semantic_model, compilation, member_id, &mut result);
            }
            LuaSemanticDeclId::TypeDecl(type_decl_id) => {
                search_type_decl_references(semantic_model, type_decl_id, &mut result);
            }
            _ => {}
        }
    } else if let Some(token) = LuaStringToken::cast(token.clone()) {
        search_string_references(semantic_model, token, &mut result);
    } else if semantic_model.get_emmyrc().references.fuzzy_search {
        fuzzy_search_references(compilation, token, &mut result);
    }

    // 简单过滤, 同行的多个引用只保留一个
    // let filtered_result = filter_duplicate_and_covered_locations(result);
    // Some(filtered_result)

    Some(result)
}

pub fn search_decl_references(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    decl_id: LuaDeclId,
    result: &mut Vec<Location>,
) -> Option<()> {
    let mut ctx = ReferenceSearchContext::default();
    search_decl_references_with_ctx(&mut ctx, semantic_model, compilation, decl_id, result)
}

fn search_decl_references_with_ctx(
    ctx: &mut ReferenceSearchContext,
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    decl_id: LuaDeclId,
    result: &mut Vec<Location>,
) -> Option<()> {
    let decl = semantic_model
        .get_db()
        .get_decl_index()
        .get_decl(&decl_id)?;
    if decl.is_local() {
        let decl_refs = semantic_model
            .get_db()
            .get_reference_index()
            .get_decl_references(&decl_id.file_id, &decl_id)?;
        let document = semantic_model.get_document();
        // 加入自己
        if let Some(location) = document.to_lsp_location(decl.get_range()) {
            result.push(location);
        }
        let typ = semantic_model.get_type(decl.get_id().into());
        let should_follow_value_alias = matches!(
            typ,
            LuaType::Signature(_)
                | LuaType::Table
                | LuaType::TableConst(_)
                | LuaType::Ref(_)
                | LuaType::Def(_)
        );

        for decl_ref in &decl_refs.cells {
            let location = document.to_lsp_location(decl_ref.range)?;
            result.push(location);
            if should_follow_value_alias {
                get_references(ctx, semantic_model, compilation, result, decl_ref);
            }
        }

        extend_module_return_value_references(ctx, semantic_model, compilation, decl_id, result);

        return Some(());
    } else {
        let name = decl.get_name();
        let global_references = semantic_model
            .get_db()
            .get_reference_index()
            .get_global_references(name)?;
        for in_filed_syntax_id in global_references {
            let document = semantic_model.get_document_by_file_id(in_filed_syntax_id.file_id)?;
            let location = document.to_lsp_location(in_filed_syntax_id.value.get_range())?;
            result.push(location);
        }
    }

    Some(())
}

pub fn search_member_references(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    member_id: LuaMemberId,
    result: &mut Vec<Location>,
) -> Option<()> {
    let mut ctx = ReferenceSearchContext::default();
    search_member_references_with_ctx(&mut ctx, semantic_model, compilation, member_id, result)
}

fn search_member_references_with_ctx(
    ctx: &mut ReferenceSearchContext,
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    member_id: LuaMemberId,
    result: &mut Vec<Location>,
) -> Option<()> {
    let member = semantic_model
        .get_db()
        .get_member_index()
        .get_member(&member_id)?;
    let key = member.get_key();
    let index_references = semantic_model
        .get_db()
        .get_reference_index()
        .get_index_references(key)?;

    let mut semantic_cache = HashMap::new();

    let semantic_id = LuaSemanticDeclId::Member(member_id);
    for in_filed_syntax_id in index_references {
        let semantic_model =
            if let Some(semantic_model) = semantic_cache.get_mut(&in_filed_syntax_id.file_id) {
                semantic_model
            } else {
                let semantic_model = compilation.get_semantic_model(in_filed_syntax_id.file_id)?;
                semantic_cache.insert(in_filed_syntax_id.file_id, semantic_model);
                semantic_cache.get_mut(&in_filed_syntax_id.file_id)?
            };
        let root = semantic_model.get_root();
        let node = in_filed_syntax_id.value.to_node_from_root(root.syntax())?;
        if semantic_model.is_reference_to(
            node.clone(),
            semantic_id.clone(),
            SemanticDeclLevel::default(),
        ) {
            let document = semantic_model.get_document();
            let range = in_filed_syntax_id.value.get_range();
            let location = document.to_lsp_location(range)?;
            result.push(location);
            search_member_secondary_references(ctx, semantic_model, compilation, node, result);
        }
    }

    Some(())
}

fn search_member_secondary_references(
    ctx: &mut ReferenceSearchContext,
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    node: LuaSyntaxNode,
    result: &mut Vec<Location>,
) -> Option<()> {
    let position = node.text_range().start();
    let parent = LuaAst::cast(node.parent()?)?;
    match parent {
        LuaAst::LuaAssignStat(assign_stat) => {
            let (vars, values) = assign_stat.get_var_and_expr_list();
            let idx = values
                .iter()
                .position(|value| value.get_position() == position)?;
            let var = vars.get(idx)?;
            let decl_id = LuaDeclId::new(semantic_model.get_file_id(), var.get_position());
            search_decl_references_with_ctx(ctx, semantic_model, compilation, decl_id, result);
            let document = semantic_model.get_document();
            let range = document.to_lsp_location(var.get_range())?;
            result.push(range);
        }
        LuaAst::LuaLocalStat(local_stat) => {
            let local_names = local_stat.get_local_name_list().collect::<Vec<_>>();
            let mut values = local_stat.get_value_exprs();
            let idx = values.position(|value| value.get_position() == position)?;
            let name = local_names.get(idx)?;
            let decl_id = LuaDeclId::new(semantic_model.get_file_id(), name.get_position());
            search_decl_references_with_ctx(ctx, semantic_model, compilation, decl_id, result);
            let document = semantic_model.get_document();
            let range = document.to_lsp_location(name.get_range())?;
            result.push(range);
        }
        _ => {}
    }

    Some(())
}

fn search_string_references(
    semantic_model: &SemanticModel,
    token: LuaStringToken,
    result: &mut Vec<Location>,
) -> Option<()> {
    let string_token_text = token.get_value();
    let string_refs = semantic_model
        .get_db()
        .get_reference_index()
        .get_string_references(&string_token_text);

    for in_filed_reference_range in string_refs {
        let document = semantic_model.get_document_by_file_id(in_filed_reference_range.file_id)?;
        let location = document.to_lsp_location(in_filed_reference_range.value)?;
        result.push(location);
    }

    Some(())
}

fn fuzzy_search_references(
    compilation: &LuaCompilation,
    token: LuaSyntaxToken,
    result: &mut Vec<Location>,
) -> Option<()> {
    let name = LuaNameToken::cast(token)?;
    let name_text = name.get_name_text();
    let fuzzy_references = compilation
        .get_db()
        .get_reference_index()
        .get_index_references(&LuaMemberKey::Name(name_text.to_string().into()))?;

    let mut semantic_cache = HashMap::new();
    for in_filed_syntax_id in fuzzy_references {
        let semantic_model =
            if let Some(semantic_model) = semantic_cache.get_mut(&in_filed_syntax_id.file_id) {
                semantic_model
            } else {
                let semantic_model = compilation.get_semantic_model(in_filed_syntax_id.file_id)?;
                semantic_cache.insert(in_filed_syntax_id.file_id, semantic_model);
                semantic_cache.get_mut(&in_filed_syntax_id.file_id)?
            };

        let document = semantic_model.get_document();
        let range = in_filed_syntax_id.value.get_range();
        let location = document.to_lsp_location(range)?;
        result.push(location);
    }

    Some(())
}

fn search_type_decl_references(
    semantic_model: &SemanticModel,
    type_decl_id: LuaTypeDeclId,
    result: &mut Vec<Location>,
) -> Option<()> {
    let refs = semantic_model
        .get_db()
        .get_reference_index()
        .get_type_references(&type_decl_id)?;
    let mut document_cache = HashMap::new();
    for in_filed_reference_range in refs {
        let document = if let Some(document) = document_cache.get(&in_filed_reference_range.file_id)
        {
            document
        } else {
            let document =
                semantic_model.get_document_by_file_id(in_filed_reference_range.file_id)?;
            document_cache.insert(in_filed_reference_range.file_id, document);
            document_cache.get(&in_filed_reference_range.file_id)?
        };
        let location = document.to_lsp_location(in_filed_reference_range.value)?;
        result.push(location);
    }

    Some(())
}

fn get_references(
    ctx: &mut ReferenceSearchContext,
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    result: &mut Vec<Location>,
    decl_ref: &DeclReferenceCell,
) -> Option<Vec<Location>> {
    let root = semantic_model.get_root();
    let position = decl_ref.range.start();
    let token = root.syntax().token_at_offset(position).right_biased()?;
    let parent = token.parent()?;

    match parent.parent()? {
        assign_stat_node if LuaAssignStat::can_cast(assign_stat_node.kind().into()) => {
            let assign_stat = LuaAssignStat::cast(assign_stat_node)?;
            let (vars, values) = assign_stat.get_var_and_expr_list();
            let idx = values
                .iter()
                .position(|value| value.get_position() == position)?;
            let var = vars.get(idx)?;
            let decl_id = semantic_model
                .find_decl(var.syntax().clone().into(), SemanticDeclLevel::default())?;
            if let LuaSemanticDeclId::Member(member_id) = decl_id {
                search_member_references_with_ctx(
                    ctx,
                    semantic_model,
                    compilation,
                    member_id,
                    result,
                );
            }
        }
        table_field_node if LuaTableField::can_cast(table_field_node.kind().into()) => {
            let table_field = LuaTableField::cast(table_field_node)?;
            let decl_id = semantic_model.find_decl(
                table_field.syntax().clone().into(),
                SemanticDeclLevel::default(),
            )?;
            if let LuaSemanticDeclId::Member(member_id) = decl_id {
                search_member_references_with_ctx(
                    ctx,
                    semantic_model,
                    compilation,
                    member_id,
                    result,
                );
            }
        }
        _ => {}
    }
    None
}

fn extend_module_return_value_references(
    ctx: &mut ReferenceSearchContext,
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    decl_id: LuaDeclId,
    result: &mut Vec<Location>,
) -> Option<()> {
    let module_file_id = decl_id.file_id;
    let module_info = semantic_model
        .get_db()
        .get_module_index()
        .get_module(module_file_id)?;
    if module_info.semantic_id.as_ref() != Some(&LuaSemanticDeclId::LuaDecl(decl_id)) {
        return Some(());
    }

    if !ctx.visited_module_exports.insert(module_file_id) {
        return Some(());
    }

    let file_dependency = semantic_model
        .get_db()
        .get_file_dependencies_index()
        .get_file_dependencies();
    let mut dependents = file_dependency.collect_file_dependents(vec![module_file_id]);
    dependents.sort();

    let mut semantic_cache: HashMap<FileId, SemanticModel> = HashMap::new();
    let mut visited_bindings: HashSet<LuaSemanticDeclId> = HashSet::new();

    for dependent_file_id in dependents {
        let dependent_semantic_model =
            if let Some(semantic_model) = semantic_cache.get_mut(&dependent_file_id) {
                semantic_model
            } else {
                let semantic_model = compilation.get_semantic_model(dependent_file_id)?;
                semantic_cache.insert(dependent_file_id, semantic_model);
                semantic_cache.get_mut(&dependent_file_id)?
            };

        let root = dependent_semantic_model.get_root();
        for node in root.descendants::<LuaAst>() {
            let LuaAst::LuaCallExpr(call_expr) = node else {
                continue;
            };

            if !call_expr.is_require() {
                continue;
            }

            if resolve_require_target_file_id(dependent_semantic_model, &call_expr)
                != Some(module_file_id)
            {
                continue;
            }

            if let Some(binding_semantic) =
                find_require_call_binding_semantic(dependent_semantic_model, &call_expr)
            {
                if !visited_bindings.insert(binding_semantic.clone()) {
                    continue;
                }

                match binding_semantic {
                    LuaSemanticDeclId::LuaDecl(decl_id) => {
                        search_decl_references_with_ctx(
                            ctx,
                            dependent_semantic_model,
                            compilation,
                            decl_id,
                            result,
                        );
                    }
                    LuaSemanticDeclId::Member(member_id) => {
                        search_member_references_with_ctx(
                            ctx,
                            dependent_semantic_model,
                            compilation,
                            member_id,
                            result,
                        );
                    }
                    _ => {}
                }
            } else {
                let document = dependent_semantic_model.get_document();
                let location = document.to_lsp_location(call_expr.get_range())?;
                result.push(location);
            }
        }
    }

    Some(())
}

fn resolve_require_target_file_id(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<FileId> {
    let args = call_expr.get_args_list()?;
    let first_arg = args.get_args().next()?;
    let require_path_type = semantic_model.infer_expr(first_arg).ok()?;
    let module_path: String = match &require_path_type {
        LuaType::StringConst(module_path) => module_path.as_ref().to_string(),
        _ => return None,
    };

    let module_info = semantic_model
        .get_db()
        .get_module_index()
        .find_module(&module_path)?;
    Some(module_info.file_id)
}

fn find_require_call_binding_semantic(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<LuaSemanticDeclId> {
    let position = call_expr.get_position();

    let mut current = call_expr.syntax().parent();
    while let Some(node) = current {
        let Some(parent) = LuaAst::cast(node.clone()) else {
            current = node.parent();
            continue;
        };

        match parent {
            LuaAst::LuaLocalStat(local_stat) => {
                let local_names = local_stat.get_local_name_list().collect::<Vec<_>>();
                let mut values = local_stat.get_value_exprs();
                let idx = values.position(|value| value.get_position() == position)?;
                let name = local_names.get(idx)?;
                return Some(LuaSemanticDeclId::LuaDecl(LuaDeclId::new(
                    semantic_model.get_file_id(),
                    name.get_position(),
                )));
            }
            LuaAst::LuaAssignStat(assign_stat) => {
                let (vars, values) = assign_stat.get_var_and_expr_list();
                let idx = values
                    .iter()
                    .position(|value| value.get_position() == position)?;
                let var = vars.get(idx)?;
                return semantic_model
                    .find_decl(var.syntax().clone().into(), SemanticDeclLevel::default());
            }
            _ => {}
        }

        current = node.parent();
    }

    None
}

#[allow(unused)]
fn filter_duplicate_and_covered_locations(locations: Vec<Location>) -> Vec<Location> {
    if locations.is_empty() {
        return locations;
    }
    let mut sorted_locations = locations;
    sorted_locations.sort_by(|a, b| {
        a.uri
            .to_string()
            .cmp(&b.uri.to_string())
            .then_with(|| a.range.start.line.cmp(&b.range.start.line))
            .then_with(|| b.range.end.line.cmp(&a.range.end.line))
    });

    let mut result = Vec::new();
    let mut seen_lines_by_uri: HashMap<String, HashSet<u32>> = HashMap::new();

    for location in sorted_locations {
        let uri_str = location.uri.to_string();
        let seen_lines = seen_lines_by_uri.entry(uri_str).or_default();

        let start_line = location.range.start.line;
        let end_line = location.range.end.line;

        let is_covered = (start_line..=end_line).any(|line| seen_lines.contains(&line));

        if !is_covered {
            for line in start_line..=end_line {
                seen_lines.insert(line);
            }
            result.push(location);
        }
    }

    // 最终按位置排序
    result.sort_by(|a, b| {
        a.uri
            .to_string()
            .cmp(&b.uri.to_string())
            .then_with(|| a.range.start.line.cmp(&b.range.start.line))
            .then_with(|| a.range.start.character.cmp(&b.range.start.character))
    });

    result
}

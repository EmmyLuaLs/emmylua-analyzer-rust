use std::collections::HashSet;

use emmylua_parser::{LuaAst, LuaAstNode, LuaAstToken, LuaCallExpr, LuaIndexExpr, LuaNameExpr};
use rowan::{NodeOrToken, TextSize};

use crate::{
    CompilationModuleInfo, DbIndex, LuaDecl, LuaDeclId, LuaInferCache, LuaSemanticDeclId,
    LuaType,
    SemanticDeclLevel, SemanticModel, infer_node_semantic_decl,
    module_query::identity::find_compilation_module_by_path,
    semantic::semantic_info::infer_token_semantic_decl,
};

pub fn enum_variable_is_param(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    index_expr: &LuaIndexExpr,
    prefix_typ: &LuaType,
) -> Option<()> {
    let LuaType::Ref(id) = prefix_typ else {
        return None;
    };

    let type_decl = db.get_type_index().get_type_decl(id)?;
    if !type_decl.is_enum() {
        return None;
    }

    let prefix_expr = index_expr.get_prefix_expr()?;
    let prefix_decl = infer_node_semantic_decl(
        db,
        cache,
        prefix_expr.syntax().clone(),
        SemanticDeclLevel::default(),
    )?;

    let LuaSemanticDeclId::LuaDecl(decl_id) = prefix_decl else {
        return None;
    };

    let mut decl_guard = DeclGuard::new();
    let origin_decl_id = find_enum_origin(db, cache, decl_id, &mut decl_guard).unwrap_or(decl_id);
    let decl = db.get_decl_index().get_decl(&origin_decl_id)?;

    if decl.is_param() { Some(()) } else { None }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclGuard {
    decl_set: HashSet<LuaDeclId>,
}

impl DeclGuard {
    pub fn new() -> Self {
        Self {
            decl_set: HashSet::new(),
        }
    }

    pub fn check(&mut self, decl_id: LuaDeclId) -> Option<()> {
        if self.decl_set.contains(&decl_id) {
            None
        } else {
            self.decl_set.insert(decl_id);
            Some(())
        }
    }
}

fn find_enum_origin(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    decl_id: LuaDeclId,
    decl_guard: &mut DeclGuard,
) -> Option<LuaDeclId> {
    decl_guard.check(decl_id)?;
    let syntax_tree = db.get_vfs().get_syntax_tree(&decl_id.file_id)?;
    let root = syntax_tree.get_red_root();

    let node = db
        .get_decl_index()
        .get_decl(&decl_id)?
        .get_value_syntax_id()?
        .to_node_from_root(&root)?;

    let semantic_decl = match node.into() {
        NodeOrToken::Node(node) => {
            infer_node_semantic_decl(db, cache, node, SemanticDeclLevel::NoTrace)
        }
        NodeOrToken::Token(token) => {
            infer_token_semantic_decl(db, cache, token, SemanticDeclLevel::NoTrace)
        }
    };

    match semantic_decl {
        Some(LuaSemanticDeclId::Member(_)) => None,
        Some(LuaSemanticDeclId::LuaDecl(new_decl_id)) => {
            let decl = db.get_decl_index().get_decl(&new_decl_id)?;
            if decl.get_value_syntax_id().is_some() {
                Some(find_enum_origin(db, cache, new_decl_id, decl_guard).unwrap_or(new_decl_id))
            } else {
                Some(new_decl_id)
            }
        }
        _ => None,
    }
}

/// 解析 require 调用表达式并获取模块信息
pub fn parse_require_module_info<'a>(
    semantic_model: &'a SemanticModel,
    decl: &LuaDecl,
) -> Option<&'a CompilationModuleInfo> {
    let call_expr = find_require_call_expr_for_local_position(semantic_model, decl.get_position())
        .or_else(|| {
            decl.get_value_syntax_id()
                .and_then(|syntax_id| {
                    semantic_model
                        .get_compilation()
                        .legacy_db()
                        .get_vfs()
                        .get_syntax_tree(&decl.get_file_id())
                        .and_then(|tree| syntax_id.to_node_from_root(&tree.get_red_root()))
                })
                .and_then(|node| {
                    LuaCallExpr::cast(node.clone())
                        .or_else(|| node.ancestors().find_map(LuaCallExpr::cast))
                })
        })?;

    resolve_require_call_module_info(semantic_model, &call_expr)
}

pub fn parse_require_module_info_by_name_expr<'a>(
    semantic_model: &'a SemanticModel,
    name_expr: &LuaNameExpr,
) -> Option<&'a CompilationModuleInfo> {
    let name_token = name_expr.get_name_token()?;
    if let Some(semantic_decl) = semantic_model.find_decl(
        NodeOrToken::Token(name_token.syntax().clone()),
        SemanticDeclLevel::NoTrace,
    ) {
        let LuaSemanticDeclId::LuaDecl(decl_id) = semantic_decl else {
            return None;
        };
        let decl = semantic_model.get_decl(&decl_id)?;
        if let Some(module_info) = parse_require_module_info(semantic_model, &decl) {
            return Some(module_info);
        }
    }

    let call_expr = find_require_call_expr_for_name_usage(
        semantic_model,
        name_token.get_name_text(),
        name_token.get_position(),
    )?;
    resolve_require_call_module_info(semantic_model, &call_expr)
}

fn find_require_call_expr_for_local_position(
    semantic_model: &SemanticModel,
    local_position: TextSize,
) -> Option<LuaCallExpr> {
    let root = semantic_model.get_root().clone();
    root.descendants::<LuaAst>().find_map(|node| match node {
        LuaAst::LuaLocalStat(local_stat) => {
            let value_expr = local_stat
                .get_local_name_list()
                .zip(local_stat.get_value_exprs())
                .find_map(|(name, value_expr)| {
                    let name_token = name.get_name_token()?;
                    (name_token.get_position() == local_position).then_some(value_expr)
                })?;

            LuaCallExpr::cast(value_expr.syntax().clone())
                .or_else(|| value_expr.syntax().ancestors().find_map(LuaCallExpr::cast))
        }
        _ => None,
    })
}

fn find_require_call_expr_for_name_usage(
    semantic_model: &SemanticModel,
    name_text: &str,
    usage_position: TextSize,
) -> Option<LuaCallExpr> {
    let root = semantic_model.get_root().clone();
    root.descendants::<LuaAst>()
        .filter_map(|node| match node {
            LuaAst::LuaLocalStat(local_stat) => local_stat
                .get_local_name_list()
                .zip(local_stat.get_value_exprs())
                .find_map(|(name, value_expr)| {
                    let name_token = name.get_name_token()?;
                    (name_token.get_name_text() == name_text
                        && name_token.get_position() <= usage_position)
                        .then_some((name_token.get_position(), value_expr))
                }),
            _ => None,
        })
        .max_by_key(|(position, _)| *position)
        .and_then(|(_, value_expr)| {
            LuaCallExpr::cast(value_expr.syntax().clone())
                .or_else(|| value_expr.syntax().ancestors().find_map(LuaCallExpr::cast))
        })
}

fn resolve_require_call_module_info<'a>(
    semantic_model: &'a SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<&'a CompilationModuleInfo> {
    if !call_expr.is_require() {
        return None;
    }
    let arg_list = call_expr.get_args_list()?;
    let first_arg = arg_list.get_args().next()?;
    let require_path_type = semantic_model.infer_expr(first_arg.clone()).ok()?;
    let module_path: String = match &require_path_type {
        LuaType::StringConst(module_path) => module_path.as_ref().to_string(),
        _ => return None,
    };

    find_compilation_module_by_path(semantic_model.get_compilation(), &module_path)
}

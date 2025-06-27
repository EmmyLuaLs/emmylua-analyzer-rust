use std::collections::HashSet;

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaIndexExpr, LuaSyntaxKind};
use rowan::NodeOrToken;

use crate::{
    infer_node_semantic_decl, semantic::semantic_info::infer_token_semantic_decl, DbIndex, LuaDecl,
    LuaDeclId, LuaInferCache, LuaSemanticDeclId, LuaType, ModuleInfo, SemanticDeclLevel,
    SemanticModel,
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

    if decl.is_param() {
        Some(())
    } else {
        None
    }
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
) -> Option<&'a ModuleInfo> {
    let value_syntax_id = decl.get_value_syntax_id()?;
    if value_syntax_id.get_kind() != LuaSyntaxKind::RequireCallExpr {
        return None;
    }

    let node = semantic_model
        .get_db()
        .get_vfs()
        .get_syntax_tree(&decl.get_file_id())
        .and_then(|tree| {
            let root = tree.get_red_root();
            semantic_model
                .get_db()
                .get_decl_index()
                .get_decl(&decl.get_id())
                .and_then(|decl| decl.get_value_syntax_id())
                .and_then(|syntax_id| syntax_id.to_node_from_root(&root))
        })?;

    let call_expr = LuaCallExpr::cast(node)?;
    let arg_list = call_expr.get_args_list()?;
    let first_arg = arg_list.get_args().next()?;
    let require_path_type = semantic_model.infer_expr(first_arg.clone()).ok()?;
    let module_path: String = match &require_path_type {
        LuaType::StringConst(module_path) => module_path.as_ref().to_string(),
        _ => {
            return None;
        }
    };

    semantic_model
        .get_db()
        .get_module_index()
        .find_module(&module_path)
}

use std::collections::HashSet;

use emmylua_code_analysis::{LuaFlowId, LuaType};
use emmylua_parser::{LuaAstNode, LuaNameExpr};

use crate::handlers::completion::{
    add_completions::{add_decl_completion, check_match_word},
    completion_builder::CompletionBuilder,
};

pub fn add_completion(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }

    let name_expr = LuaNameExpr::cast(builder.trigger_token.parent()?)?;
    let file_id = builder.semantic_model.get_file_id();
    let decl_tree = builder
        .semantic_model
        .get_db()
        .get_decl_index()
        .get_decl_tree(&file_id)?;

    let mut duplicated_name = HashSet::new();
    let local_env = decl_tree.get_env_decls(builder.trigger_token.text_range().start())?;
    let flow_id = LuaFlowId::from_node(name_expr.syntax());
    for decl_id in local_env.iter() {
        let (name, mut typ) = {
            let decl = builder
                .semantic_model
                .get_db()
                .get_decl_index()
                .get_decl(&decl_id)?;
            (
                decl.get_name().to_string(),
                decl.get_type().cloned().unwrap_or(LuaType::Unknown),
            )
        };
        if duplicated_name.contains(&name) {
            continue;
        }
        if !check_match_word(builder.trigger_token.text(), name.as_str()) {
            duplicated_name.insert(name.clone());
            continue;
        }

        if let Some(chain) = builder
            .semantic_model
            .get_db()
            .get_flow_index()
            .get_flow_chain(file_id, flow_id)
        {
            let semantic_model = &builder.semantic_model;
            let db = semantic_model.get_db();
            let root = semantic_model.get_root().syntax();
            let config = semantic_model.get_config();
            for type_assert in chain.get_type_asserts(&name, name_expr.get_position()) {
                typ = type_assert.tighten_type(db, &mut config.borrow_mut(), root, typ)?;
            }
        }

        duplicated_name.insert(name.clone());
        add_decl_completion(builder, decl_id.clone(), &name, &typ);
    }

    let global_env = builder
        .semantic_model
        .get_db()
        .get_decl_index()
        .get_global_decls();
    for decl_id in global_env.iter() {
        let decl = builder
            .semantic_model
            .get_db()
            .get_decl_index()
            .get_decl(&decl_id)?;
        let (name, typ) = {
            (
                decl.get_name().to_string(),
                decl.get_type().cloned().unwrap_or(LuaType::Unknown),
            )
        };
        if duplicated_name.contains(&name) {
            continue;
        }
        if !check_match_word(builder.trigger_token.text(), name.as_str()) {
            duplicated_name.insert(name.clone());
            continue;
        }
        // 如果范围相同, 则是在定义一个新的全局变量, 不需要添加
        if decl.get_range() == builder.trigger_token.text_range() {
            continue;
        }

        duplicated_name.insert(name.clone());
        add_decl_completion(builder, decl_id.clone(), &name, &typ);
    }

    builder.env_duplicate_name.extend(duplicated_name);

    Some(())
}

use std::collections::HashSet;

use emmylua_code_analysis::{LuaFlowId, LuaType};
use emmylua_parser::{LuaAst, LuaAstNode};

use crate::handlers::completion::{
    add_completions::{add_decl_completion, check_match_word},
    completion_builder::CompletionBuilder,
};

pub fn add_completion(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }
    let node = LuaAst::cast(builder.trigger_token.parent()?)?;
    match node {
        LuaAst::LuaNameExpr(_) => {}
        LuaAst::LuaBlock(_) => {}
        LuaAst::LuaCallArgList(_) => {}
        // 在表中无键时主动触发的补全, 不在这里处理
        // TODO 如果左值类型不是类而是纯表则允许
        LuaAst::LuaTableExpr(_)|
        // 字符串中触发的补全
        LuaAst::LuaLiteralExpr(_)  => return None,
        _ => {},
    };

    let mut duplicated_name = HashSet::new();
    add_local_env(builder, &mut duplicated_name, &node);
    add_global_env(builder, &mut duplicated_name);

    builder.env_duplicate_name.extend(duplicated_name);

    Some(())
}

fn add_local_env(
    builder: &mut CompletionBuilder,
    duplicated_name: &mut HashSet<String>,
    node: &LuaAst,
) -> Option<()> {
    let flow_id = LuaFlowId::from_node(node.syntax());

    let file_id = builder.semantic_model.get_file_id();
    let decl_tree = builder
        .semantic_model
        .get_db()
        .get_decl_index()
        .get_decl_tree(&file_id)?;
    let local_env = decl_tree.get_env_decls(builder.trigger_token.text_range().start())?;

    let trigger_text = builder.get_trigger_text();

    for decl_id in local_env.iter() {
        // 获取变量名和类型
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

        if !env_check_match_word(&trigger_text, name.as_str()) {
            duplicated_name.insert(name.clone());
            continue;
        }

        // 类型缩窄
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
            for type_assert in
                chain.get_type_asserts(&name, node.get_position(), Some(decl_id.position))
            {
                typ = type_assert
                    .tighten_type(db, &mut config.borrow_mut(), root, typ)
                    .unwrap_or(LuaType::Unknown);
            }
        }

        duplicated_name.insert(name.clone());
        add_decl_completion(builder, decl_id.clone(), &name, &typ);
    }

    Some(())
}

fn add_global_env(
    builder: &mut CompletionBuilder,
    duplicated_name: &mut HashSet<String>,
) -> Option<()> {
    let trigger_text = builder.get_trigger_text();
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
        if !env_check_match_word(&trigger_text, name.as_str()) {
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

    Some(())
}

fn env_check_match_word(trigger_text: &str, name: &str) -> bool {
    // 如果首字母是`(`或者`,`则允许, 用于在函数参数调用处触发补全
    if trigger_text.starts_with('(') || trigger_text.starts_with(',') {
        return true;
    }
    check_match_word(trigger_text, name)
}

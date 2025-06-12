use emmylua_parser::{BinaryOperator, LuaAstNode, LuaAstToken, LuaComment, LuaDocTag};

use crate::{
    compilation::analyzer::flow::{
        binder::FlowBinder,
        flow_node::{FlowAssertion, FlowId, FlowNodeKind},
    },
    InFiled, LuaType, LuaVarRefId,
};

enum CastAction {
    Add,
    Remove,
    Force,
}

pub fn bind_comment(
    binder: &mut FlowBinder,
    lua_comment: LuaComment,
    mut current: FlowId,
) -> FlowId {
    let cast_tags = lua_comment.get_doc_tags().filter_map(|it| match it {
        LuaDocTag::Cast(cast) => Some(cast),
        _ => None,
    });

    for cast_tag in cast_tags {
        let Some(name_token) = cast_tag.get_name_token() else {
            continue;
        };
        let name = name_token.get_name_text();
        let decl = binder
            .db
            .get_decl_index()
            .get_decl_tree(&binder.file_id)
            .and_then(|decl_tree| decl_tree.find_local_decl(&name, name_token.get_position()));
        let var_ref_id = match decl {
            Some(decl) => LuaVarRefId::DeclId(decl.get_id()),
            None => LuaVarRefId::Name(name.into()),
        };

        let mut assertions = vec![];
        for cast_op_type in cast_tag.get_op_types() {
            let action = match cast_op_type.get_op() {
                Some(op) => {
                    if op.get_op() == BinaryOperator::OpAdd {
                        CastAction::Add
                    } else {
                        CastAction::Remove
                    }
                }
                None => CastAction::Force,
            };

            if cast_op_type.is_nullable() {
                match action {
                    CastAction::Add => {
                        assertions.push(FlowAssertion::TypeAdd(var_ref_id.clone(), LuaType::Nil));
                    }
                    CastAction::Remove => {
                        assertions
                            .push(FlowAssertion::TypeRemove(var_ref_id.clone(), LuaType::Nil));
                    }
                    _ => {}
                }
            } else if let Some(doc_typ) = cast_op_type.get_type() {
                let file_id = binder.file_id;
                let key = InFiled::new(file_id, doc_typ.get_syntax_id());
                let typ = match binder.context.cast_flow.get(&key) {
                    Some(t) => t.clone(),
                    None => continue,
                };

                match action {
                    CastAction::Add => {
                        assertions.push(FlowAssertion::TypeAdd(var_ref_id.clone(), typ.clone()));
                    }
                    CastAction::Remove => {
                        assertions.push(FlowAssertion::TypeRemove(var_ref_id.clone(), typ.clone()));
                    }
                    CastAction::Force => {
                        assertions.push(FlowAssertion::TypeForce(var_ref_id.clone(), typ.clone()));
                    }
                }
            }
        }

        for assertion in assertions {
            let flow_assertion = assertion.clone();
            let flow_id = binder.create_node(FlowNodeKind::Assertion(flow_assertion.into()));
            binder.add_antecedent(flow_id, current);
            current = flow_id;
        }
    }

    current
}

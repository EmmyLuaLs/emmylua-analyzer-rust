use emmylua_parser::{LuaAstNode, LuaComment, LuaDocTag, LuaDocTagAs, LuaDocTagCast};

use super::{FlowEffect, FlowId, FlowNodeKind, binder::FlowBinder, exprs::bind_expr};

pub fn bind_comment(binder: &mut FlowBinder, lua_comment: LuaComment, current: FlowId) -> FlowId {
    let mut parent = current;
    for tag in lua_comment.get_doc_tags() {
        parent = match tag {
            LuaDocTag::Cast(cast) => bind_cast_tag(binder, lua_comment.clone(), cast, parent),
            LuaDocTag::As(as_tag) => bind_as_tag(binder, lua_comment.clone(), as_tag, parent),
            _ => parent,
        };
    }

    parent
}

fn bind_cast_tag(
    binder: &mut FlowBinder,
    lua_comment: LuaComment,
    cast: LuaDocTagCast,
    current: FlowId,
) -> FlowId {
    let expr = cast.get_key_expr();
    if let Some(expr) = expr {
        bind_expr(binder, expr, current);

        let flow_id = binder.create_node(FlowNodeKind::TagCast(cast.to_ptr()));
        binder.add_effect(flow_id, FlowEffect::TagCast(cast.to_ptr()));
        binder.add_antecedent(flow_id, current);
        flow_id
    } else {
        bind_inline_cast_owner(
            binder,
            lua_comment,
            FlowNodeKind::TagCast(cast.to_ptr()),
            FlowEffect::TagCast(cast.to_ptr()),
            current,
        )
    }
}

fn bind_as_tag(
    binder: &mut FlowBinder,
    lua_comment: LuaComment,
    as_tag: LuaDocTagAs,
    current: FlowId,
) -> FlowId {
    bind_inline_cast_owner(
        binder,
        lua_comment,
        FlowNodeKind::AsCast(as_tag.to_ptr()),
        FlowEffect::AsCast(as_tag.to_ptr()),
        current,
    )
}

fn bind_inline_cast_owner(
    binder: &mut FlowBinder,
    lua_comment: LuaComment,
    kind: FlowNodeKind,
    effect: FlowEffect,
    current: FlowId,
) -> FlowId {
    let Some(owner) = lua_comment.get_owner() else {
        return current;
    };

    let flow_id = binder.create_node(kind);
    binder.add_effect(flow_id, effect);
    if let Some(bind_flow) = binder.get_bind_flow(owner.get_syntax_id()) {
        binder.add_antecedent(flow_id, bind_flow);
        binder.bind_syntax_node(owner.get_syntax_id(), flow_id);
    } else {
        binder.add_antecedent(flow_id, current);
        binder.bind_syntax_node(owner.get_syntax_id(), flow_id);
    }
    flow_id
}

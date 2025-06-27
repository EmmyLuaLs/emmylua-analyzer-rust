use emmylua_parser::{LuaAstNode, LuaComment, LuaDocTag};

use crate::{compilation::analyzer::flow::binder::FlowBinder, FlowId, FlowNodeKind};

pub fn bind_comment(binder: &mut FlowBinder, lua_comment: LuaComment, current: FlowId) -> FlowId {
    let cast_tags = lua_comment.get_doc_tags().filter_map(|it| match it {
        LuaDocTag::Cast(cast) => Some(cast),
        _ => None,
    });

    let mut parent = current;
    for cast in cast_tags {
        let flow_id = binder.create_node(FlowNodeKind::TagCast(cast.to_ptr()));
        binder.add_antecedent(flow_id, parent);
        parent = flow_id;
    }

    parent
}

use emmylua_parser::{LuaAstNode, LuaComment, LuaDocTag, LuaDocTagCast};

use crate::compilation::analyzer::{flow::binder::FlowBinder, lua};

pub fn bind_comment(binder: &mut FlowBinder, lua_comment: LuaComment) -> Option<()> {
    let cast_tags = lua_comment.get_doc_tags().filter_map(|it| match it {
        LuaDocTag::Cast(cast) => Some(cast),
        _ => None,
    });

    for cast in cast_tags {
        todo!()
    }

    None
}

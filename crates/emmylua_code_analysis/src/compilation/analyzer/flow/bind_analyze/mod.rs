use emmylua_parser::{LuaAst, LuaAstNode, LuaChunk};

use crate::compilation::analyzer::flow::binder::FlowBinder;

pub fn bind_analyze(binder: &mut FlowBinder, chunk: LuaChunk) -> Option<()> {
    // todo!()
    for node in chunk.descendants::<LuaAst>() {
        match node {
            LuaAst::LuaBlock(block) => {
                // todo!("Analyze LuaBlock: {:?}", block);
            }
            _ => {}
        }
    }

    Some(())
}

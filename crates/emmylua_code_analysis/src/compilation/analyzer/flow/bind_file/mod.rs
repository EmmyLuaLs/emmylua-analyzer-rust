use emmylua_parser::LuaChunk;

use crate::compilation::analyzer::flow::binder::FlowBinder;

pub fn bind_file(_: &mut FlowBinder, _: LuaChunk) -> Option<()> {
    // todo!()
    Some(())
}

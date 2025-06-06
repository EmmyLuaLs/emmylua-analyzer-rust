use emmylua_parser::{LuaAst, LuaLocalStat};

use crate::compilation::analyzer::flow::{bind_analyze::bind_each_child, binder::FlowBinder};

pub fn bind_local_stat(binder: &mut FlowBinder, local_stat: LuaLocalStat) -> Option<()> {
    let saved_current = binder.current;
    bind_each_child(binder, LuaAst::LuaLocalStat(local_stat));
    binder.current = saved_current;

    Some(())
}

pub fn bind_assign_stat(binder: &mut FlowBinder, assign_stat: LuaAst) -> Option<()> {
    let saved_current = binder.current;
    bind_each_child(binder, assign_stat);
    binder.current = saved_current;

    

    Some(())
}
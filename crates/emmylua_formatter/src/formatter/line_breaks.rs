use emmylua_parser::LuaChunk;

use super::FormatContext;
use super::model::RootFormatPlan;

pub fn analyze_root_line_breaks(
    ctx: &FormatContext,
    _chunk: &LuaChunk,
    mut plan: RootFormatPlan,
) -> RootFormatPlan {
    plan.line_breaks.insert_final_newline = ctx.config.output.insert_final_newline;
    plan
}

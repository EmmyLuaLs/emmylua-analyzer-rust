mod expr;
mod layout;
mod line_breaks;
mod model;
mod render;
mod sequence;
mod spacing;
mod trivia;

use crate::config::LuaFormatConfig;
use crate::ir::DocIR;
use emmylua_parser::LuaChunk;

pub struct FormatContext<'a> {
    pub config: &'a LuaFormatConfig,
}

impl<'a> FormatContext<'a> {
    pub fn new(config: &'a LuaFormatConfig) -> Self {
        Self { config }
    }
}

pub fn format_chunk(ctx: &FormatContext, chunk: &LuaChunk) -> Vec<DocIR> {
    let spacing_plan = spacing::analyze_root_spacing(ctx, chunk);
    let layout_plan = layout::analyze_root_layout(ctx, chunk, spacing_plan);
    let final_plan = line_breaks::analyze_root_line_breaks(ctx, chunk, layout_plan);
    render::render_root(ctx, chunk, &final_plan)
}

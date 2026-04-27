mod expr;
mod layout;
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

    render::render_root(ctx, chunk, &layout_plan)
}

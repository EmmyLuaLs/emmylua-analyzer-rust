mod expr;
mod layout;
mod model;
mod render;
mod sequence;
mod spacing;
#[cfg(test)]
mod test;
mod trivia;

use crate::config::LuaFormatConfig;
use crate::formatter::model::FormatPlan;
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
    let mut plan = FormatPlan::from_config(ctx.config);
    spacing::analyze_spacing(ctx, chunk, &mut plan);
    layout::analyze_layout(ctx, chunk, &mut plan);

    render::render(ctx, chunk, &plan)
}

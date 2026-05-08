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
use crate::formatter::model::RootFormatPlan;
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
    let mut plan = RootFormatPlan::from_config(ctx.config);
    spacing::analyze_root_spacing(ctx, chunk, &mut plan);
    layout::analyze_root_layout(ctx, chunk, &mut plan);

    render::render_root(ctx, chunk, &plan)
}

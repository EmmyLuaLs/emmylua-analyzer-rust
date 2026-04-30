mod expr;
mod layout;
mod model;
mod render;
mod sequence;
mod spacing;
mod trivia;

use std::time::Duration;

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

#[derive(Debug, Clone, Copy, Default)]
pub struct FormatPhaseProfile {
    pub spacing: Duration,
    pub layout: Duration,
    pub render: Duration,
}

pub fn format_chunk(ctx: &FormatContext, chunk: &LuaChunk) -> Vec<DocIR> {
    let spacing_plan = spacing::analyze_root_spacing(ctx, chunk);
    let layout_plan = layout::analyze_root_layout(ctx, chunk, spacing_plan);

    render::render_root(ctx, chunk, &layout_plan)
}

pub fn format_chunk_with_profile(
    ctx: &FormatContext,
    chunk: &LuaChunk,
) -> (Vec<DocIR>, FormatPhaseProfile) {
    let spacing_start = std::time::Instant::now();
    let spacing_plan = spacing::analyze_root_spacing(ctx, chunk);
    let spacing = spacing_start.elapsed();

    let layout_start = std::time::Instant::now();
    let layout_plan = layout::analyze_root_layout(ctx, chunk, spacing_plan);
    let layout = layout_start.elapsed();

    let render_start = std::time::Instant::now();
    let docs = render::render_root(ctx, chunk, &layout_plan);
    let render = render_start.elapsed();

    (
        docs,
        FormatPhaseProfile {
            spacing,
            layout,
            render,
        },
    )
}

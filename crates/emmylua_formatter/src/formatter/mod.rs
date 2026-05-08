mod expr;
mod layout;
mod model;
mod render;
mod sequence;
mod spacing;
#[cfg(test)]
mod test;
mod trivia;

use std::cell::RefCell;
use std::fmt::Write;
use std::time::{Duration, Instant};

use crate::config::LuaFormatConfig;
use crate::formatter::model::FormatPlan;
use crate::ir::DocIR;
use emmylua_parser::LuaChunk;

thread_local! {
    static RENDER_HOTSPOT_PROFILE: RefCell<Option<RenderHotspotProfile>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FormatPhaseProfile {
    pub spacing: Duration,
    pub layout: Duration,
    pub render: Duration,
}

impl FormatPhaseProfile {
    pub fn summary(&self) -> String {
        format!(
            "formatter-phases spacing_ms={:.3} layout_ms={:.3} render_ms={:.3}",
            self.spacing.as_secs_f64() * 1000.0,
            self.layout.as_secs_f64() * 1000.0,
            self.render.as_secs_f64() * 1000.0,
        )
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderHotspotEntry {
    pub total: Duration,
    pub calls: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderHotspotProfile {
    pub call_args: RenderHotspotEntry,
    pub call_arg_values: RenderHotspotEntry,
    pub call_args_from_docs: RenderHotspotEntry,
    pub chain_expr: RenderHotspotEntry,
    pub chain_collect_segments: RenderHotspotEntry,
    pub chain_from_segments: RenderHotspotEntry,
    pub source_line_prefix_width: RenderHotspotEntry,
}

impl RenderHotspotProfile {
    pub fn summary(&self) -> String {
        let mut text = String::from("formatter-render-hotspots");
        append_hotspot_summary(&mut text, "call_args", self.call_args);
        append_hotspot_summary(&mut text, "call_arg_values", self.call_arg_values);
        append_hotspot_summary(&mut text, "call_args_from_docs", self.call_args_from_docs);
        append_hotspot_summary(&mut text, "chain_expr", self.chain_expr);
        append_hotspot_summary(&mut text, "chain_collect_segments", self.chain_collect_segments);
        append_hotspot_summary(&mut text, "chain_from_segments", self.chain_from_segments);
        append_hotspot_summary(
            &mut text,
            "source_line_prefix_width",
            self.source_line_prefix_width,
        );
        text
    }
}

fn append_hotspot_summary(text: &mut String, label: &str, entry: RenderHotspotEntry) {
    let _ = write!(
        text,
        " {}_ms={:.3} {}_calls={}",
        label,
        entry.total.as_secs_f64() * 1000.0,
        label,
        entry.calls,
    );
}

#[derive(Clone, Copy)]
pub enum RenderHotspotKind {
    CallArgs,
    CallArgValues,
    CallArgsFromDocs,
    ChainExpr,
    ChainCollectSegments,
    ChainFromSegments,
    SourceLinePrefixWidth,
}

pub struct FormatContext<'a> {
    pub config: &'a LuaFormatConfig,
}

impl<'a> FormatContext<'a> {
    pub fn new(config: &'a LuaFormatConfig) -> Self {
        Self { config }
    }
}

pub fn format_chunk(ctx: &FormatContext, chunk: &LuaChunk) -> Vec<DocIR> {
    format_chunk_with_profile(ctx, chunk).0
}

pub fn format_chunk_with_profile(
    ctx: &FormatContext,
    chunk: &LuaChunk,
) -> (Vec<DocIR>, FormatPhaseProfile, RenderHotspotProfile) {
    let mut plan = FormatPlan::from_config(ctx.config);
    reset_render_hotspot_profile(true);

    let spacing_start = Instant::now();
    spacing::analyze_spacing(ctx, chunk, &mut plan);
    let spacing = spacing_start.elapsed();

    let layout_start = Instant::now();
    layout::analyze_layout(ctx, chunk, &mut plan);
    let layout = layout_start.elapsed();

    let render_start = Instant::now();
    let docs = render::render_ir(ctx, chunk, &plan);
    let render = render_start.elapsed();

    (
        docs,
        FormatPhaseProfile {
            spacing,
            layout,
            render,
        },
        take_render_hotspot_profile(),
    )
}

pub(crate) fn profile_render_hotspot<T>(kind: RenderHotspotKind, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let result = f();
    let elapsed = start.elapsed();
    RENDER_HOTSPOT_PROFILE.with(|slot| {
        if let Some(profile) = &mut *slot.borrow_mut() {
            let entry = match kind {
                RenderHotspotKind::CallArgs => &mut profile.call_args,
                RenderHotspotKind::CallArgValues => &mut profile.call_arg_values,
                RenderHotspotKind::CallArgsFromDocs => &mut profile.call_args_from_docs,
                RenderHotspotKind::ChainExpr => &mut profile.chain_expr,
                RenderHotspotKind::ChainCollectSegments => &mut profile.chain_collect_segments,
                RenderHotspotKind::ChainFromSegments => &mut profile.chain_from_segments,
                RenderHotspotKind::SourceLinePrefixWidth => &mut profile.source_line_prefix_width,
            };
            entry.total += elapsed;
            entry.calls += 1;
        }
    });
    result
}

fn reset_render_hotspot_profile(enabled: bool) {
    RENDER_HOTSPOT_PROFILE.with(|slot| {
        *slot.borrow_mut() = enabled.then_some(RenderHotspotProfile::default());
    });
}

fn take_render_hotspot_profile() -> RenderHotspotProfile {
    RENDER_HOTSPOT_PROFILE.with(|slot| slot.borrow_mut().take().unwrap_or_default())
}

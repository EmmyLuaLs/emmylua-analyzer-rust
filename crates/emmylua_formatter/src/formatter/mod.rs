mod expr;
mod layout;
mod model;
mod render;
mod sequence;
mod spacing;
mod trivia;

use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use std::time::Instant;

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

const RENDER_HOTSPOT_KIND_COUNT: usize = 5;

#[derive(Debug, Clone, Default)]
pub struct RenderHotspotProfile {
    pub calls: [u64; RENDER_HOTSPOT_KIND_COUNT],
    pub total_ns: [u64; RENDER_HOTSPOT_KIND_COUNT],
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RenderHotspotKind {
    StatementExpr,
    CallArgs,
    TableExpr,
    ChainExpr,
    BinaryChain,
}

impl RenderHotspotKind {
    fn index(self) -> usize {
        match self {
            RenderHotspotKind::StatementExpr => 0,
            RenderHotspotKind::CallArgs => 1,
            RenderHotspotKind::TableExpr => 2,
            RenderHotspotKind::ChainExpr => 3,
            RenderHotspotKind::BinaryChain => 4,
        }
    }

    fn label(self) -> &'static str {
        match self {
            RenderHotspotKind::StatementExpr => "statement_expr",
            RenderHotspotKind::CallArgs => "call_args",
            RenderHotspotKind::TableExpr => "table_expr",
            RenderHotspotKind::ChainExpr => "chain_expr",
            RenderHotspotKind::BinaryChain => "binary_chain",
        }
    }
}

#[derive(Default)]
struct RenderHotspotProfileState {
    enabled: bool,
    profile: RenderHotspotProfile,
}

fn render_hotspot_profile_state() -> &'static Mutex<RenderHotspotProfileState> {
    static STATE: OnceLock<Mutex<RenderHotspotProfileState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(RenderHotspotProfileState::default()))
}

pub(crate) fn reset_render_hotspot_profile(enabled: bool) {
    let mut state = render_hotspot_profile_state().lock().unwrap();
    state.enabled = enabled;
    state.profile = RenderHotspotProfile::default();
}

pub(crate) fn take_render_hotspot_profile() -> RenderHotspotProfile {
    let mut state = render_hotspot_profile_state().lock().unwrap();
    let profile = std::mem::take(&mut state.profile);
    state.enabled = false;
    profile
}

pub(crate) fn profile_render_hotspot<T>(kind: RenderHotspotKind, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let result = f();
    let elapsed = start.elapsed().as_nanos() as u64;
    let mut state = render_hotspot_profile_state().lock().unwrap();
    if state.enabled {
        let index = kind.index();
        state.profile.calls[index] += 1;
        state.profile.total_ns[index] += elapsed;
    }
    result
}

impl RenderHotspotProfile {
    pub fn summary(&self) -> String {
        let mut segments = Vec::new();
        for kind in [
            RenderHotspotKind::StatementExpr,
            RenderHotspotKind::CallArgs,
            RenderHotspotKind::TableExpr,
            RenderHotspotKind::ChainExpr,
            RenderHotspotKind::BinaryChain,
        ] {
            let index = kind.index();
            let calls = self.calls[index];
            if calls == 0 {
                continue;
            }
            segments.push(format!(
                "{}{{calls={},time_ms={:.3}}}",
                kind.label(),
                calls,
                self.total_ns[index] as f64 / 1_000_000.0,
            ));
        }

        format!("render_hotspots {}", segments.join(" "))
    }
}

#[derive(Debug, Clone, Default)]
pub struct FormatPhaseProfile {
    pub spacing: Duration,
    pub layout: Duration,
    pub render: Duration,
    pub sequence: sequence::SequenceProfile,
    pub render_hotspots: RenderHotspotProfile,
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
    sequence::reset_sequence_profile(true);
    reset_render_hotspot_profile(true);
    let spacing_start = std::time::Instant::now();
    let spacing_plan = spacing::analyze_root_spacing(ctx, chunk);
    let spacing = spacing_start.elapsed();

    let layout_start = std::time::Instant::now();
    let layout_plan = layout::analyze_root_layout(ctx, chunk, spacing_plan);
    let layout = layout_start.elapsed();

    let render_start = std::time::Instant::now();
    let docs = render::render_root(ctx, chunk, &layout_plan);
    let render = render_start.elapsed();
    let sequence = sequence::take_sequence_profile();
    let render_hotspots = take_render_hotspot_profile();

    (
        docs,
        FormatPhaseProfile {
            spacing,
            layout,
            render,
            sequence,
            render_hotspots,
        },
    )
}

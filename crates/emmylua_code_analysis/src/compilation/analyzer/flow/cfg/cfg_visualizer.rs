use emmylua_parser::LuaAstNode;

use super::{BasicBlock, ControlFlowGraph, EdgeKind};
use std::fmt::Write;

/// CFG可视化器，用于生成DOT格式的图形描述
pub struct CfgVisualizer;

impl CfgVisualizer {
    /// 将CFG转换为DOT格式字符串
    pub fn to_dot(cfg: &ControlFlowGraph) -> String {
        let mut dot = String::new();

        writeln!(dot, "digraph CFG {{").unwrap();
        writeln!(dot, "  rankdir=TB;").unwrap();
        writeln!(dot, "  node [shape=box];").unwrap();

        // 输出所有基本块
        for (id, block) in &cfg.blocks {
            let label = Self::format_block_label(block);
            let style = Self::get_block_style(block);
            writeln!(dot, "  {} [label=\"{}\" {}];", id.0, label, style).unwrap();
        }

        // 输出所有边
        for edge in &cfg.edges {
            let edge_style = Self::get_edge_style(&edge.kind);
            let label = Self::get_edge_label(&edge.kind);
            writeln!(
                dot,
                "  {} -> {} [label=\"{}\" {}];",
                edge.from.0, edge.to.0, label, edge_style
            )
            .unwrap();
        }

        writeln!(dot, "}}").unwrap();
        dot
    }

    /// 格式化基本块标签
    fn format_block_label(block: &BasicBlock) -> String {
        let mut label = format!("Block {}", block.id.0);

        if block.is_entry {
            label.push_str("\\n(Entry)");
        }
        if block.is_exit {
            label.push_str("\\n(Exit)");
        }

        if !block.statements.is_empty() {
            label.push_str("\\n---\\n");
            for (i, stmt) in block.statements.iter().enumerate() {
                if i < 3 {
                    // 只显示前3个语句
                    let stmt_text = stmt
                        .get_text()
                        .replace("\\", "\\\\")
                        .replace("\"", "\\\"")
                        .replace("\n", "\\n");
                    if stmt_text.len() > 30 {
                        label.push_str(&format!("{}...\\n", &stmt_text[..27]));
                    } else {
                        label.push_str(&format!("{}\\n", stmt_text));
                    }
                } else if i == 3 {
                    label.push_str("...\\n");
                    break;
                }
            }
        }

        label
    }

    /// 获取基本块样式
    fn get_block_style(block: &BasicBlock) -> String {
        if block.is_entry {
            "style=filled fillcolor=lightgreen".to_string()
        } else if block.is_exit {
            "style=filled fillcolor=lightcoral".to_string()
        } else {
            "style=filled fillcolor=lightblue".to_string()
        }
    }

    /// 获取边的标签
    fn get_edge_label(kind: &EdgeKind) -> &'static str {
        match kind {
            EdgeKind::Unconditional => "",
            EdgeKind::ConditionalTrue => "true",
            EdgeKind::ConditionalFalse => "false",
            EdgeKind::Break => "break",
            EdgeKind::Continue => "continue",
            EdgeKind::Return => "return",
            EdgeKind::Exception => "exception",
        }
    }

    /// 获取边的样式
    fn get_edge_style(kind: &EdgeKind) -> String {
        match kind {
            EdgeKind::Unconditional => "color=black".to_string(),
            EdgeKind::ConditionalTrue => "color=green".to_string(),
            EdgeKind::ConditionalFalse => "color=red".to_string(),
            EdgeKind::Break => "color=orange style=dashed".to_string(),
            EdgeKind::Continue => "color=blue style=dashed".to_string(),
            EdgeKind::Return => "color=purple style=bold".to_string(),
            EdgeKind::Exception => "color=red style=dotted".to_string(),
        }
    }

    /// 生成文本形式的CFG描述
    pub fn to_text(cfg: &ControlFlowGraph) -> String {
        let mut text = String::new();

        writeln!(text, "Control Flow Graph:").unwrap();
        writeln!(text, "==================").unwrap();

        if let Some(entry_id) = cfg.entry_block {
            writeln!(text, "Entry Block: {}", entry_id.0).unwrap();
        }
        if let Some(exit_id) = cfg.exit_block {
            writeln!(text, "Exit Block: {}", exit_id.0).unwrap();
        }
        writeln!(text).unwrap();

        // 按ID排序输出基本块
        let mut sorted_blocks: Vec<_> = cfg.blocks.iter().collect();
        sorted_blocks.sort_by_key(|(id, _)| id.0);

        for (id, block) in sorted_blocks {
            writeln!(text, "Block {}:", id.0).unwrap();
            if block.is_entry {
                writeln!(text, "  (Entry Block)").unwrap();
            }
            if block.is_exit {
                writeln!(text, "  (Exit Block)").unwrap();
            }

            writeln!(
                text,
                "  Predecessors: {:?}",
                block.predecessors.iter().map(|id| id.0).collect::<Vec<_>>()
            )
            .unwrap();
            writeln!(
                text,
                "  Successors: {:?}",
                block.successors.iter().map(|id| id.0).collect::<Vec<_>>()
            )
            .unwrap();

            if !block.statements.is_empty() {
                writeln!(text, "  Statements:").unwrap();
                for (i, stmt) in block.statements.iter().enumerate() {
                    let stmt_text = stmt.get_text();
                    writeln!(text, "    {}: {}", i, stmt_text.trim()).unwrap();
                }
            }
            writeln!(text).unwrap();
        }

        if !cfg.edges.is_empty() {
            writeln!(text, "Edges:").unwrap();
            for edge in &cfg.edges {
                writeln!(text, "  {} -> {} ({:?})", edge.from.0, edge.to.0, edge.kind).unwrap();
            }
        }

        text
    }
}

/// CFG统计信息
#[derive(Debug)]
pub struct CfgStats {
    pub total_blocks: usize,
    pub total_edges: usize,
    pub entry_blocks: usize,
    pub exit_blocks: usize,
    pub max_predecessors: usize,
    pub max_successors: usize,
}

impl CfgStats {
    pub fn analyze(cfg: &ControlFlowGraph) -> Self {
        let total_blocks = cfg.blocks.len();
        let total_edges = cfg.edges.len();
        let entry_blocks = cfg.blocks.values().filter(|b| b.is_entry).count();
        let exit_blocks = cfg.blocks.values().filter(|b| b.is_exit).count();

        let max_predecessors = cfg
            .blocks
            .values()
            .map(|b| b.predecessors.len())
            .max()
            .unwrap_or(0);

        let max_successors = cfg
            .blocks
            .values()
            .map(|b| b.successors.len())
            .max()
            .unwrap_or(0);

        Self {
            total_blocks,
            total_edges,
            entry_blocks,
            exit_blocks,
            max_predecessors,
            max_successors,
        }
    }
}

use emmylua_parser::{LuaAstNode, LuaChunk, LuaSyntaxId, LuaSyntaxKind, LuaSyntaxNode};

use crate::formatter_new::model::{CommentLayoutPlan, LayoutNodePlan, SyntaxNodeLayoutPlan};

pub fn collect_root_layout_nodes(chunk: &LuaChunk) -> Vec<LayoutNodePlan> {
    collect_child_layout_nodes(chunk.syntax())
}

fn collect_child_layout_nodes(node: &LuaSyntaxNode) -> Vec<LayoutNodePlan> {
    node.children().filter_map(collect_layout_node).collect()
}

fn collect_layout_node(node: LuaSyntaxNode) -> Option<LayoutNodePlan> {
    match node.kind().into() {
        LuaSyntaxKind::Comment => Some(LayoutNodePlan::Comment(collect_comment_layout(node))),
        kind => Some(LayoutNodePlan::Syntax(SyntaxNodeLayoutPlan {
            syntax_id: LuaSyntaxId::from_node(&node),
            kind,
            children: collect_child_layout_nodes(&node),
        })),
    }
}

fn collect_comment_layout(node: LuaSyntaxNode) -> CommentLayoutPlan {
    CommentLayoutPlan {
        syntax_id: LuaSyntaxId::from_node(&node),
    }
}

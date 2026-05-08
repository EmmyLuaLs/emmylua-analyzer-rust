use hashbrown::HashSet;

use emmylua_parser::{
    LuaAstNode, LuaChunk, LuaComment, LuaCommentFormatDirective, LuaSyntaxId, LuaSyntaxKind,
    LuaSyntaxNode,
};

use crate::formatter::model::{CommentLayoutPlan, LayoutNodePlan, SyntaxNodeLayoutPlan};

pub fn collect_root_layout_nodes(
    chunk: &LuaChunk,
    format_disabled: &mut HashSet<LuaSyntaxId>,
) -> Vec<LayoutNodePlan> {
    collect_child_layout_nodes(chunk.syntax(), format_disabled)
}

fn collect_child_layout_nodes(
    node: &LuaSyntaxNode,
    format_disabled: &mut HashSet<LuaSyntaxId>,
) -> Vec<LayoutNodePlan> {
    let mut nodes = Vec::new();
    let mut format_off = false;

    for child in node.children() {
        let syntax_id = LuaSyntaxId::from_node(&child);

        if let Some(comment) = LuaComment::cast(child.clone()) {
            let directive = comment.get_format_directive();
            if format_off || directive.is_some() {
                format_disabled.insert(syntax_id);
            }

            if let Some(layout_node) = collect_layout_node(child, format_disabled) {
                nodes.push(layout_node);
            }

            match directive {
                Some(LuaCommentFormatDirective::FormatOff) => format_off = true,
                Some(LuaCommentFormatDirective::FormatOn) => format_off = false,
                None => {}
            }
            continue;
        }

        if format_off {
            format_disabled.insert(syntax_id);
        }

        if let Some(layout_node) = collect_layout_node(child, format_disabled) {
            nodes.push(layout_node);
        }
    }

    nodes
}

fn collect_layout_node(
    node: LuaSyntaxNode,
    format_disabled: &mut HashSet<LuaSyntaxId>,
) -> Option<LayoutNodePlan> {
    match node.kind().into() {
        LuaSyntaxKind::Comment => Some(LayoutNodePlan::Comment(collect_comment_layout(node))),
        kind => Some(LayoutNodePlan::Syntax(SyntaxNodeLayoutPlan {
            syntax_id: LuaSyntaxId::from_node(&node),
            kind,
            children: collect_child_layout_nodes(&node, format_disabled),
        })),
    }
}

fn collect_comment_layout(node: LuaSyntaxNode) -> CommentLayoutPlan {
    CommentLayoutPlan {
        syntax_id: LuaSyntaxId::from_node(&node),
    }
}

use std::collections::HashMap;

use emmylua_parser::{LuaSyntaxId, LuaSyntaxKind};

use crate::config::LuaFormatConfig;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenSpacingExpected {
    Space(usize),
    #[allow(unused)]
    MaxSpace(usize),
}

#[derive(Clone, Debug, Default)]
pub struct RootSpacingModel {
    pub has_shebang: bool,
    left_expected: HashMap<LuaSyntaxId, TokenSpacingExpected>,
    right_expected: HashMap<LuaSyntaxId, TokenSpacingExpected>,
    replace_tokens: HashMap<LuaSyntaxId, String>,
}

impl RootSpacingModel {
    pub fn add_token_left_expected(
        &mut self,
        syntax_id: LuaSyntaxId,
        expected: TokenSpacingExpected,
    ) {
        self.left_expected.insert(syntax_id, expected);
    }

    pub fn add_token_right_expected(
        &mut self,
        syntax_id: LuaSyntaxId,
        expected: TokenSpacingExpected,
    ) {
        self.right_expected.insert(syntax_id, expected);
    }

    pub fn left_expected(&self, syntax_id: LuaSyntaxId) -> Option<&TokenSpacingExpected> {
        self.left_expected.get(&syntax_id)
    }

    pub fn right_expected(&self, syntax_id: LuaSyntaxId) -> Option<&TokenSpacingExpected> {
        self.right_expected.get(&syntax_id)
    }

    pub fn add_token_replace(&mut self, syntax_id: LuaSyntaxId, replacement: String) {
        self.replace_tokens.insert(syntax_id, replacement);
    }

    pub fn token_replace(&self, syntax_id: LuaSyntaxId) -> Option<&str> {
        self.replace_tokens.get(&syntax_id).map(String::as_str)
    }
}

#[derive(Clone, Debug)]
pub struct SyntaxNodeLayoutPlan {
    pub syntax_id: LuaSyntaxId,
    pub kind: LuaSyntaxKind,
    pub children: Vec<LayoutNodePlan>,
}

#[derive(Clone, Debug)]
pub struct CommentLayoutPlan {
    pub syntax_id: LuaSyntaxId,
}

#[derive(Clone, Debug)]
pub enum LayoutNodePlan {
    Syntax(SyntaxNodeLayoutPlan),
    Comment(CommentLayoutPlan),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatementTriviaLayoutPlan {
    pub has_inline_comment: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ControlHeaderLayoutPlan {
    pub has_inline_comment: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatementExprListLayoutKind {
    Sequence,
    PreserveFirstMultiline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatementExprListLayoutPlan {
    pub kind: StatementExprListLayoutKind,
    pub first_line_prefix_width: usize,
    pub attach_single_value_head: bool,
    pub allow_fill: bool,
    pub allow_packed: bool,
    pub allow_one_per_line: bool,
    pub prefer_balanced_break_lines: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExprSequenceLayoutPlan {
    pub first_line_prefix_width: usize,
    pub preserve_multiline: bool,
}

#[derive(Clone, Debug, Default)]
pub struct RootLayoutModel {
    pub format_block_with_legacy: bool,
    pub root_nodes: Vec<LayoutNodePlan>,
    pub statement_trivia: HashMap<LuaSyntaxId, StatementTriviaLayoutPlan>,
    pub statement_expr_lists: HashMap<LuaSyntaxId, StatementExprListLayoutPlan>,
    pub expr_sequences: HashMap<LuaSyntaxId, ExprSequenceLayoutPlan>,
    pub control_headers: HashMap<LuaSyntaxId, ControlHeaderLayoutPlan>,
    pub control_header_expr_lists: HashMap<LuaSyntaxId, StatementExprListLayoutPlan>,
}

#[derive(Clone, Debug, Default)]
pub struct RootLineBreakModel {
    pub insert_final_newline: bool,
}

#[derive(Clone, Debug, Default)]
pub struct RootFormatPlan {
    pub spacing: RootSpacingModel,
    pub layout: RootLayoutModel,
    pub line_breaks: RootLineBreakModel,
}

impl RootFormatPlan {
    pub fn from_config(config: &LuaFormatConfig) -> Self {
        Self {
            spacing: RootSpacingModel::default(),
            layout: RootLayoutModel {
                format_block_with_legacy: true,
                root_nodes: Vec::new(),
                statement_trivia: HashMap::new(),
                statement_expr_lists: HashMap::new(),
                expr_sequences: HashMap::new(),
                control_headers: HashMap::new(),
                control_header_expr_lists: HashMap::new(),
            },
            line_breaks: RootLineBreakModel {
                insert_final_newline: config.output.insert_final_newline,
            },
        }
    }
}

use std::collections::HashMap;

use emmylua_parser::{LuaAstNode, LuaComment, LuaSyntaxId, LuaTokenKind};

use crate::formatter::comments::TokenExpected;
use crate::ir::{self, DocIR};

pub struct CommentFormatter {
    left_expected: HashMap<LuaSyntaxId, TokenExpected>,
    right_expected: HashMap<LuaSyntaxId, TokenExpected>,
    replace_tokens: HashMap<LuaSyntaxId, String>,
}

impl CommentFormatter {
    pub fn new() -> Self {
        Self {
            left_expected: HashMap::new(),
            right_expected: HashMap::new(),
            replace_tokens: HashMap::new(),
        }
    }

    pub fn add_token_left_expected(&mut self, syntax_id: LuaSyntaxId, expected: TokenExpected) {
        self.left_expected.insert(syntax_id, expected);
    }

    pub fn add_token_right_expected(&mut self, syntax_id: LuaSyntaxId, expected: TokenExpected) {
        self.right_expected.insert(syntax_id, expected);
    }

    pub fn get_left_expected(&self, syntax_id: LuaSyntaxId) -> Option<&TokenExpected> {
        self.left_expected.get(&syntax_id)
    }

    pub fn get_right_expected(&self, syntax_id: LuaSyntaxId) -> Option<&TokenExpected> {
        self.right_expected.get(&syntax_id)
    }

    pub fn add_token_replace(&mut self, syntax_id: LuaSyntaxId, replacement: String) {
        self.replace_tokens.insert(syntax_id, replacement);
    }

    pub fn get_token_replace(&self, syntax_id: LuaSyntaxId) -> Option<&str> {
        self.replace_tokens.get(&syntax_id).map(String::as_str)
    }

    pub fn render_comment(&self, comment: &LuaComment) -> Vec<DocIR> {
        self.render_comment_lines(comment)
            .into_iter()
            .enumerate()
            .flat_map(|(index, line)| {
                let mut docs = Vec::new();
                if index > 0 {
                    docs.push(ir::hard_line());
                }
                if !line.is_empty() {
                    docs.push(ir::text(line));
                }
                docs
            })
            .collect()
    }

    pub fn render_comment_text(&self, comment: &LuaComment) -> String {
        let mut lines = self.render_comment_lines(comment).into_iter();
        let Some(first_line) = lines.next() else {
            return String::new();
        };

        if lines.len() == 0 {
            return first_line;
        }

        let mut rendered = first_line;
        for line in lines {
            rendered.push('\n');
            rendered.push_str(&line);
        }

        rendered
    }

    fn render_comment_lines(&self, comment: &LuaComment) -> Vec<String> {
        let mut lines = Vec::new();
        let mut current_line = String::new();
        let mut prev_token_id = None;
        let mut pending_gap = String::new();
        let mut line_has_content = false;
        let mut ended_with_newline = false;

        for element in comment.syntax().descendants_with_tokens() {
            let Some(token) = element.into_token() else {
                continue;
            };

            match token.kind().into() {
                LuaTokenKind::TkWhitespace => {
                    pending_gap.push_str(token.text());
                }
                LuaTokenKind::TkEndOfLine => {
                    lines.push(std::mem::take(&mut current_line));
                    prev_token_id = None;
                    pending_gap.clear();
                    line_has_content = false;
                    ended_with_newline = true;
                }
                _ => {
                    let syntax_id = LuaSyntaxId::from_token(&token);
                    if line_has_content {
                        current_line.push_str(&self.resolve_gap(
                            prev_token_id,
                            syntax_id,
                            &pending_gap,
                        ));
                    }

                    current_line.push_str(
                        self.get_token_replace(syntax_id)
                            .unwrap_or_else(|| token.text()),
                    );
                    line_has_content = true;
                    prev_token_id = Some(syntax_id);
                    pending_gap.clear();
                    ended_with_newline = false;
                }
            }
        }

        if line_has_content || ended_with_newline {
            lines.push(std::mem::take(&mut current_line));
        }

        lines
    }

    fn resolve_gap(
        &self,
        prev_token_id: Option<LuaSyntaxId>,
        token_id: LuaSyntaxId,
        gap: &str,
    ) -> String {
        let mut exact_space = None;
        let mut max_space = None;

        if let Some(prev_token_id) = prev_token_id
            && let Some(expected) = self.get_right_expected(prev_token_id)
        {
            match expected {
                TokenExpected::Space(count) => exact_space = Some(*count),
                TokenExpected::MaxSpace(count) => max_space = Some(*count),
            }
        }

        if let Some(expected) = self.get_left_expected(token_id) {
            match expected {
                TokenExpected::Space(count) => {
                    exact_space = Some(exact_space.map_or(*count, |current| current.max(*count)));
                }
                TokenExpected::MaxSpace(count) => {
                    max_space = Some(max_space.map_or(*count, |current| current.min(*count)));
                }
            }
        }

        if let Some(exact_space) = exact_space {
            return " ".repeat(exact_space);
        }

        if let Some(max_space) = max_space {
            let original_space_count = gap.chars().take_while(|ch| *ch == ' ').count();
            return " ".repeat(original_space_count.min(max_space));
        }

        gap.to_string()
    }
}

mod formatter_context;
mod syntax_node_change;

use std::collections::HashMap;

use emmylua_parser::{LuaAst, LuaAstNode, LuaSyntaxId, LuaTokenKind};
use rowan::NodeOrToken;

use crate::format::formatter_context::FormatterContext;
pub use crate::format::syntax_node_change::{TokenExpected, TokenNodeChange};

#[allow(unused)]
#[derive(Debug)]
pub struct LuaFormatter {
    root: LuaAst,
    token_changes: HashMap<LuaSyntaxId, TokenNodeChange>,
    token_left_expected: HashMap<LuaSyntaxId, TokenExpected>,
    token_right_expected: HashMap<LuaSyntaxId, TokenExpected>,
}

#[allow(unused)]
impl LuaFormatter {
    pub fn new(root: LuaAst) -> Self {
        Self {
            root,
            token_changes: HashMap::new(),
            token_left_expected: HashMap::new(),
            token_right_expected: HashMap::new(),
        }
    }

    pub fn add_token_change(&mut self, syntax_id: LuaSyntaxId, change: TokenNodeChange) {
        self.token_changes.insert(syntax_id, change);
    }

    pub fn add_token_left_expected(&mut self, syntax_id: LuaSyntaxId, expected: TokenExpected) {
        self.token_left_expected.insert(syntax_id, expected);
    }

    pub fn add_token_right_expected(&mut self, syntax_id: LuaSyntaxId, expected: TokenExpected) {
        self.token_right_expected.insert(syntax_id, expected);
    }

    pub fn get_token_change(&self, syntax_id: &LuaSyntaxId) -> Option<&TokenNodeChange> {
        self.token_changes.get(syntax_id)
    }

    pub fn get_root(&self) -> LuaAst {
        self.root.clone()
    }

    pub fn get_formatted_text(&self) -> String {
        let mut context = FormatterContext::new();
        for node_or_token in self.root.syntax().descendants_with_tokens() {
            if let NodeOrToken::Token(token) = node_or_token {
                let token_kind = token.kind().to_token();
                match (context.current_expected.take(), token_kind) {
                    (Some(TokenExpected::Space(n)), LuaTokenKind::TkWhitespace) => {
                        if !context.is_line_first_token {
                            context.text.push_str(&" ".repeat(n));
                            continue;
                        }
                    }
                    (Some(TokenExpected::MaxSpace(n)), LuaTokenKind::TkWhitespace) => {
                        if !context.is_line_first_token {
                            let white_space_len = token.text().chars().count();
                            if white_space_len > n {
                                context.reset_whitespace_to(n);
                                continue;
                            }
                        }
                    }
                    (_, LuaTokenKind::TkEndOfLine) => {
                        // No space expected
                        context.reset_whitespace();
                        context.text.push('\n');
                        context.is_line_first_token = true;
                        continue;
                    }
                    (Some(TokenExpected::Space(n)), _) => {
                        if !context.is_line_first_token {
                            context.text.push_str(&" ".repeat(n));
                        }
                    }
                    _ => {}
                }

                let syntax_id = LuaSyntaxId::from_token(&token);
                if let Some(expected) = self.token_left_expected.get(&syntax_id) {
                    match expected {
                        TokenExpected::Space(n) => {
                            if !context.is_line_first_token {
                                context.reset_whitespace();
                                context.text.push_str(&" ".repeat(*n));
                            }
                        }
                        TokenExpected::MaxSpace(n) => {
                            if !context.is_line_first_token {
                                let current_spaces = context.get_last_whitespace_count();
                                if current_spaces > *n {
                                    context.reset_whitespace_to(*n);
                                }
                            }
                        }
                    }
                }

                if token_kind != LuaTokenKind::TkWhitespace {
                    context.is_line_first_token = false;
                }

                if let Some(change) = self.token_changes.get(&syntax_id) {
                    match change {
                        TokenNodeChange::Remove => continue,
                        TokenNodeChange::AddLeft(s) => {
                            context.text.push_str(s);
                            context.text.push_str(token.text());
                        }
                        TokenNodeChange::AddRight(s) => {
                            context.text.push_str(token.text());
                            context.text.push_str(s);
                        }
                        TokenNodeChange::ReplaceWith(s) => {
                            context.text.push_str(s);
                        }
                    }
                } else {
                    context.text.push_str(token.text());
                }

                context.current_expected = self.token_right_expected.get(&syntax_id).cloned();
            }
        }

        context.text
    }
}

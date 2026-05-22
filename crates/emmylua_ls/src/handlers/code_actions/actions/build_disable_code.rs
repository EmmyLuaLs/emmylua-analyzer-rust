use std::collections::HashMap;

use emmylua_code_analysis::{DiagnosticCode, Emmyrc, LuaDocument, SemanticModel};
use emmylua_formatter::resolve_config_for_path;
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaComment, LuaCommentOwner, LuaDocTag, LuaDocTagDiagnostic, LuaExpr,
    LuaKind, LuaStat, LuaSyntaxNode, LuaTokenKind,
};
use lsp_types::{Position, Range, TextEdit, Uri};
use rowan::{TextSize, TokenAtOffset};

use crate::handlers::command::DisableAction;

#[derive(Debug, Clone)]
enum DisableLineAst {
    Stat(LuaStat),
    Expr(LuaExpr),
}

impl DisableLineAst {
    fn get_left_comment(&self) -> Option<LuaComment> {
        match self {
            DisableLineAst::Stat(stat) => stat.get_left_comment(),
            DisableLineAst::Expr(expr) => {
                if let Some(attached_comment) = find_expr_attached_comment(expr.syntax()) {
                    return LuaComment::cast(attached_comment);
                }
                None
            }
        }
    }

    fn get_position(&self) -> TextSize {
        match self {
            DisableLineAst::Stat(stat) => stat.get_position(),
            DisableLineAst::Expr(expr) => expr.get_position(),
        }
    }

    fn syntax(&self) -> &LuaSyntaxNode {
        match self {
            DisableLineAst::Stat(stat) => stat.syntax(),
            DisableLineAst::Expr(expr) => expr.syntax(),
        }
    }
}

fn find_expr_attached_comment(node: &LuaSyntaxNode) -> Option<LuaSyntaxNode> {
    let mut prev_sibling = node.prev_sibling_or_token();
    while let Some(sibling) = prev_sibling {
        match sibling.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace) => {}
            LuaKind::Token(LuaTokenKind::TkEndOfLine) => match sibling.prev_sibling_or_token() {
                Some(prev_sibling) => {
                    return prev_sibling.clone().into_node();
                }
                _ => {
                    return None;
                }
            },
            _ => {
                return None;
            }
        }
        prev_sibling = sibling.prev_sibling_or_token();
    }
    None
}

pub fn build_disable_next_line_changes(
    semantic_model: &SemanticModel<'_>,
    start: Position,
    code: DiagnosticCode,
) -> Option<HashMap<Uri, Vec<TextEdit>>> {
    let emmyrc = semantic_model.get_emmyrc();
    let document = semantic_model.get_document();
    let offset = document.get_offset(start.line as usize, start.character as usize)?;
    let root = semantic_model.get_root();
    if offset >= root.get_range().end() {
        return None;
    }

    let token = match root.syntax().token_at_offset(offset) {
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::Between(_, token) => token,
        _ => return None,
    };

    let stat = token.parent_ancestors().find_map(LuaStat::cast)?;
    let mut ast = DisableLineAst::Stat(stat.clone());
    let expr = token.parent_ancestors().find_map(LuaExpr::cast);
    // 如果 expr 是 stat 的子节点, 则认为是 expr
    if let Some(expr) = expr
        && stat.get_range().contains(expr.get_range().start())
    {
        let stat_line = semantic_model
            .get_document()
            .get_line(stat.get_position())?;
        let expr_line = semantic_model
            .get_document()
            .get_line(expr.get_position())?;
        if expr_line != stat_line {
            ast = DisableLineAst::Expr(expr);
        }
    };

    let mut text_edit = None;

    if let Some(comment) = ast.get_left_comment() {
        if let Some(diagnostic_tag) =
            find_diagnostic_disable_tag(comment.clone(), DisableAction::Line)
        {
            let new_start = if let Some(actions_list) = diagnostic_tag.get_code_list() {
                actions_list.get_range().end()
            } else {
                diagnostic_tag.get_range().end()
            };

            let (line, col) = document.get_line_col(new_start)?;
            text_edit = Some(TextEdit {
                range: Range {
                    start: Position {
                        line: line as u32,
                        character: col as u32,
                    },
                    end: Position {
                        line: line as u32,
                        character: col as u32,
                    },
                },
                new_text: format!(", {}", code.get_name()),
            });
        } else {
            text_edit = get_disable_next_line_text_edit(
                &document,
                emmyrc,
                comment.syntax().clone(),
                comment.get_position(),
                code,
            );
        }
    };

    if text_edit.is_none() {
        text_edit = get_disable_next_line_text_edit(
            &document,
            emmyrc,
            ast.syntax().clone(),
            ast.get_position(),
            code,
        );
    }

    #[allow(clippy::mutable_key_type)]
    let mut changes = HashMap::new();
    let uri = document.get_uri();
    changes.insert(uri, vec![text_edit?]);

    Some(changes)
}

fn get_disable_next_line_text_edit(
    document: &LuaDocument,
    emmyrc: &Emmyrc,
    node: LuaSyntaxNode,
    offset: TextSize,
    code: DiagnosticCode,
) -> Option<TextEdit> {
    let indent_text = if let Some(prefix_token) = node.prev_sibling_or_token() {
        if prefix_token.kind() == LuaTokenKind::TkWhitespace.into() {
            prefix_token.into_token()?.text().to_string()
        } else {
            "".to_string()
        }
    } else {
        "".to_string()
    };

    let line = document.get_line(offset)?;
    let space = if code_action_insert_space(emmyrc, document) {
        " "
    } else {
        ""
    };
    Some(TextEdit {
        range: Range {
            start: Position {
                line: line as u32,
                character: 0,
            },
            end: Position {
                line: line as u32,
                character: 0,
            },
        },
        new_text: format!(
            "{}---{}@diagnostic disable-next-line: {}\n",
            indent_text,
            space,
            code.get_name()
        ),
    })
}

pub fn build_disable_file_changes(
    semantic_model: &SemanticModel<'_>,
    code: DiagnosticCode,
) -> Option<HashMap<Uri, Vec<TextEdit>>> {
    let root = semantic_model.get_root();
    let first_block = root.get_block()?;
    let first_child = first_block.children::<LuaAst>().next()?;
    let document = semantic_model.get_document();
    let emmyrc = semantic_model.get_emmyrc();
    let space = if code_action_insert_space(emmyrc, &document) {
        " "
    } else {
        ""
    };
    let text_edit = if let LuaAst::LuaComment(comment) = first_child {
        if let Some(diagnostic_tag) =
            find_diagnostic_disable_tag(comment.clone(), DisableAction::File)
        {
            let new_start = if let Some(actions_list) = diagnostic_tag.get_code_list() {
                actions_list.get_range().end()
            } else {
                diagnostic_tag.get_range().end()
            };

            let (line, col) = document.get_line_col(new_start)?;
            TextEdit {
                range: Range {
                    start: Position {
                        line: line as u32,
                        character: col as u32,
                    },
                    end: Position {
                        line: line as u32,
                        character: col as u32,
                    },
                },
                new_text: format!(", {}", code.get_name()),
            }
        } else {
            TextEdit {
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 0,
                    },
                },
                new_text: format!("---{}@diagnostic disable: {}\n", space, code.get_name()),
            }
        }
    } else {
        TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 0,
                },
            },
            new_text: format!("---{}@diagnostic disable: {}\n", space, code.get_name()),
        }
    };

    #[allow(clippy::mutable_key_type)]
    let mut changes = HashMap::new();
    let uri = document.get_uri();
    changes.insert(uri, vec![text_edit]);

    Some(changes)
}

fn code_action_insert_space(emmyrc: &Emmyrc, document: &LuaDocument) -> bool {
    emmyrc.code_action.insert_space.unwrap_or_else(|| {
        resolve_config_for_path(Some(document.get_file_path().as_path()), None)
            .map(|resolved| resolved.config.emmy_doc.space_between_tag_columns)
            .unwrap_or(false)
    })
}

fn find_diagnostic_disable_tag(
    comment: LuaComment,
    action: DisableAction,
) -> Option<LuaDocTagDiagnostic> {
    let diagnostic_tags = comment.get_doc_tags().filter_map(|tag| {
        if let LuaDocTag::Diagnostic(diagnostic) = tag {
            Some(diagnostic)
        } else {
            None
        }
    });

    for diagnostic_tag in diagnostic_tags {
        let action_token = diagnostic_tag.get_action_token()?;
        let action_token_text = action_token.get_name_text();
        match action {
            DisableAction::Line => {
                if action_token_text == "disable-next-line" {
                    return Some(diagnostic_tag);
                }
            }
            DisableAction::File | DisableAction::Project => {
                if action_token_text == "disable" {
                    return Some(diagnostic_tag);
                }
            }
        }
    }
    None
}

use emmylua_code_analysis::{SalsaDatabase, SalsaSemanticModel, SemanticId};
use emmylua_parser::{
    LuaAstNode, LuaDocNameType, LuaSyntaxKind, LuaSyntaxNode, LuaSyntaxToken, LuaTokenKind,
};
use lsp_types::{DocumentHighlight, DocumentHighlightKind};
use rowan::NodeOrToken;

use crate::handlers::common::{
    decl_reference_ranges, member_reference_ranges, type_def_reference_ranges,
};

pub fn highlight_tokens(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    token: LuaSyntaxToken,
) -> Option<Vec<DocumentHighlight>> {
    let mut result = Vec::new();
    // Type doc name (`---@type Foo` / `---@param x Foo`): highlight same-name type definitions and uses.
    if let Some(name_type) = token.parent_ancestors().find_map(LuaDocNameType::cast) {
        if let Some(name) = name_type.get_name_text()
            && let Some(def) = model.resolve_type_def(&name)
        {
            for (file_id, range) in type_def_reference_ranges(salsa, &def, true) {
                if file_id != model.file_id() {
                    continue;
                }
                push_highlight(
                    model,
                    salsa,
                    range,
                    Some(DocumentHighlightKind::TEXT),
                    &mut result,
                );
            }
            return Some(result);
        }
    }
    match token.kind().into() {
        LuaTokenKind::TkName => {
            let Some(decl) = model.find_decl(token.clone().into()) else {
                highlight_name(model, salsa, token, &mut result);
                return Some(result);
            };
            match &decl {
                SemanticId::Decl(_) => highlight_decl_references(model, salsa, &decl, &mut result),
                SemanticId::Member(_) => {
                    for (file_id, range) in member_reference_ranges(salsa, &decl, true) {
                        if file_id != model.file_id() {
                            continue;
                        }
                        push_highlight(model, salsa, range, None, &mut result);
                    }
                }
                _ => {
                    let _ = highlight_name(model, salsa, token, &mut result);
                }
            }
        }
        token_kind if is_keyword(token_kind) => {
            highlight_keywords(model, salsa, token, &mut result);
        }
        _ => {}
    }

    Some(result)
}

fn highlight_decl_references(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    decl: &SemanticId,
    result: &mut Vec<DocumentHighlight>,
) {
    let ranges = decl_reference_ranges(salsa, decl, false);
    for (file_id, range) in ranges {
        // document_highlight only cares about the current file.
        if file_id != model.file_id() {
            continue;
        }
        push_highlight(model, salsa, range, None, result);
    }
    // Declaration name (write position): only handle declarations in the current file to avoid converting ranges from other files into this document.
    if let SemanticId::Decl(key) = decl
        && key.file_id == model.file_id()
    {
        push_highlight(
            model,
            salsa,
            key.name_range,
            Some(DocumentHighlightKind::WRITE),
            result,
        );
    }
}

fn highlight_name(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    token: LuaSyntaxToken,
    result: &mut Vec<DocumentHighlight>,
) -> Option<()> {
    let root = model.chunk()?;
    let token_name = token.text();
    for node_or_token in root.syntax().descendants_with_tokens() {
        if let NodeOrToken::Token(token) = node_or_token
            && token.kind() == LuaTokenKind::TkName.into()
            && token.text() == token_name
        {
            push_highlight(
                model,
                salsa,
                token.text_range(),
                Some(DocumentHighlightKind::TEXT),
                result,
            );
        }
    }

    Some(())
}

fn is_keyword(kind: LuaTokenKind) -> bool {
    matches!(
        kind,
        LuaTokenKind::TkAnd
            | LuaTokenKind::TkBreak
            | LuaTokenKind::TkDo
            | LuaTokenKind::TkElse
            | LuaTokenKind::TkElseIf
            | LuaTokenKind::TkEnd
            | LuaTokenKind::TkFor
            | LuaTokenKind::TkFunction
            | LuaTokenKind::TkGoto
            | LuaTokenKind::TkIf
            | LuaTokenKind::TkIn
            | LuaTokenKind::TkLocal
            | LuaTokenKind::TkRepeat
            | LuaTokenKind::TkReturn
            | LuaTokenKind::TkThen
            | LuaTokenKind::TkUntil
            | LuaTokenKind::TkWhile
    )
}

fn highlight_keywords(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    token: LuaSyntaxToken,
    result: &mut Vec<DocumentHighlight>,
) -> Option<()> {
    let parent_node = token.parent()?;
    match parent_node.kind().into() {
        LuaSyntaxKind::LocalFuncStat | LuaSyntaxKind::FuncStat => {
            highlight_node_keywords(model, salsa, parent_node.clone(), result);
            let closure_node = parent_node
                .children()
                .find(|node| node.kind() == LuaSyntaxKind::ClosureExpr.into())?;
            highlight_node_keywords(model, salsa, closure_node, result);
        }
        _ => {
            highlight_node_keywords(model, salsa, parent_node, result);
        }
    }

    Some(())
}

fn highlight_node_keywords(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    node: LuaSyntaxNode,
    result: &mut Vec<DocumentHighlight>,
) -> Option<()> {
    for node_or_token in node.children_with_tokens() {
        if let NodeOrToken::Token(token) = node_or_token
            && is_keyword(token.kind().into())
        {
            push_highlight(
                model,
                salsa,
                token.text_range(),
                Some(DocumentHighlightKind::TEXT),
                result,
            );
        }
    }

    Some(())
}

fn push_highlight(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    range: rowan::TextRange,
    kind: Option<DocumentHighlightKind>,
    result: &mut Vec<DocumentHighlight>,
) {
    let Some(document) = salsa.document(model.file_id()) else {
        return;
    };
    let Some(lsp_range) = document.to_lsp_range(range) else {
        return;
    };
    if result.iter().any(|h| h.range == lsp_range) {
        return;
    }
    result.push(DocumentHighlight {
        range: lsp_range,
        kind,
    });
}

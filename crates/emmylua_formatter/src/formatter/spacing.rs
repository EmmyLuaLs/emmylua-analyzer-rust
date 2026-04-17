use crate::config::LuaFormatConfig;
use crate::ir::{self, DocIR};
use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaChunk, LuaKind, LuaSyntaxId, LuaSyntaxKind, LuaSyntaxToken,
    LuaTokenKind,
};

use super::FormatContext;
use super::model::{RootFormatPlan, RootSpacingModel, TokenSpacingExpected};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpaceRule {
    Space,
    NoSpace,
}

impl SpaceRule {
    pub(crate) fn to_ir(self) -> DocIR {
        match self {
            SpaceRule::Space => ir::space(),
            SpaceRule::NoSpace => ir::list(vec![]),
        }
    }
}

pub(crate) fn space_around_binary_op(op: BinaryOperator, config: &LuaFormatConfig) -> SpaceRule {
    match op {
        BinaryOperator::OpAdd
        | BinaryOperator::OpSub
        | BinaryOperator::OpMul
        | BinaryOperator::OpDiv
        | BinaryOperator::OpIDiv
        | BinaryOperator::OpMod
        | BinaryOperator::OpPow => {
            if config.spacing.space_around_math_operator {
                SpaceRule::Space
            } else {
                SpaceRule::NoSpace
            }
        }
        BinaryOperator::OpEq
        | BinaryOperator::OpNe
        | BinaryOperator::OpLt
        | BinaryOperator::OpGt
        | BinaryOperator::OpLe
        | BinaryOperator::OpGe
        | BinaryOperator::OpAnd
        | BinaryOperator::OpOr
        | BinaryOperator::OpBAnd
        | BinaryOperator::OpBOr
        | BinaryOperator::OpBXor
        | BinaryOperator::OpShl
        | BinaryOperator::OpShr
        | BinaryOperator::OpNop => SpaceRule::Space,
        BinaryOperator::OpConcat => {
            if config.spacing.space_around_concat_operator {
                SpaceRule::Space
            } else {
                SpaceRule::NoSpace
            }
        }
    }
}

pub(crate) fn space_around_assign(config: &LuaFormatConfig) -> SpaceRule {
    if config.spacing.space_around_assign_operator {
        SpaceRule::Space
    } else {
        SpaceRule::NoSpace
    }
}

pub fn analyze_root_spacing(ctx: &FormatContext, chunk: &LuaChunk) -> RootFormatPlan {
    let mut plan = RootFormatPlan::from_config(ctx.config);
    plan.spacing.has_shebang = chunk
        .syntax()
        .first_token()
        .is_some_and(|token| token.kind() == LuaKind::Token(LuaTokenKind::TkShebang));

    analyze_chunk_token_spacing(ctx, chunk, &mut plan.spacing);

    plan
}

fn analyze_chunk_token_spacing(
    ctx: &FormatContext,
    chunk: &LuaChunk,
    spacing: &mut RootSpacingModel,
) {
    for element in chunk.syntax().descendants_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };

        if should_skip_spacing_token(&token) {
            continue;
        }

        analyze_token_spacing(ctx, spacing, &token);
    }
}

fn should_skip_spacing_token(token: &LuaSyntaxToken) -> bool {
    matches!(
        token.kind().to_token(),
        LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine | LuaTokenKind::TkShebang
    )
}

fn analyze_token_spacing(
    ctx: &FormatContext,
    spacing: &mut RootSpacingModel,
    token: &LuaSyntaxToken,
) {
    let syntax_id = LuaSyntaxId::from_token(token);
    match token.kind().to_token() {
        LuaTokenKind::TkNormalStart => apply_comment_start_spacing(ctx, spacing, token, syntax_id),
        LuaTokenKind::TkDocStart => {
            spacing.add_token_replace(syntax_id, normalized_doc_tag_prefix(ctx));
            spacing.add_token_right_expected(syntax_id, TokenSpacingExpected::Space(0));
        }
        LuaTokenKind::TkDocContinue => {
            spacing.add_token_replace(syntax_id, normalized_doc_continue_prefix(ctx, token.text()));
            spacing.add_token_right_expected(syntax_id, TokenSpacingExpected::Space(0));
        }
        LuaTokenKind::TkDocContinueOr => {
            spacing.add_token_replace(
                syntax_id,
                normalized_doc_continue_or_prefix(ctx, token.text()),
            );
            spacing.add_token_right_expected(syntax_id, TokenSpacingExpected::Space(0));
        }
        LuaTokenKind::TkLeftParen => apply_left_paren_spacing(ctx, spacing, token, syntax_id),
        LuaTokenKind::TkRightParen => apply_right_paren_spacing(ctx, spacing, token, syntax_id),
        LuaTokenKind::TkLeftBracket => apply_left_bracket_spacing(ctx, spacing, token, syntax_id),
        LuaTokenKind::TkRightBracket => {
            spacing.add_token_left_expected(
                syntax_id,
                TokenSpacingExpected::Space(space_inside_brackets(token, ctx)),
            );
        }
        LuaTokenKind::TkLeftBrace => {
            spacing.add_token_right_expected(
                syntax_id,
                TokenSpacingExpected::Space(space_inside_braces(token, ctx)),
            );
        }
        LuaTokenKind::TkRightBrace => {
            spacing.add_token_left_expected(
                syntax_id,
                TokenSpacingExpected::Space(space_inside_braces(token, ctx)),
            );
        }
        LuaTokenKind::TkComma => {
            spacing.add_token_left_expected(syntax_id, TokenSpacingExpected::Space(0));
            spacing.add_token_right_expected(syntax_id, TokenSpacingExpected::Space(1));
        }
        LuaTokenKind::TkSemicolon => {
            spacing.add_token_left_expected(syntax_id, TokenSpacingExpected::Space(0));
            spacing.add_token_right_expected(syntax_id, TokenSpacingExpected::Space(1));
        }
        LuaTokenKind::TkColon => {
            if is_parent_syntax(token, LuaSyntaxKind::IndexExpr) {
                spacing.add_token_left_expected(syntax_id, TokenSpacingExpected::Space(0));
                spacing.add_token_right_expected(syntax_id, TokenSpacingExpected::Space(0));
            } else if in_comment(token) {
                spacing.add_token_left_expected(syntax_id, TokenSpacingExpected::Space(0));
                spacing.add_token_right_expected(syntax_id, TokenSpacingExpected::Space(1));
            }
        }
        LuaTokenKind::TkDot => {
            spacing.add_token_left_expected(syntax_id, TokenSpacingExpected::Space(0));
            spacing.add_token_right_expected(syntax_id, TokenSpacingExpected::Space(0));
        }
        LuaTokenKind::TkPlus | LuaTokenKind::TkMinus => {
            if is_parent_syntax(token, LuaSyntaxKind::UnaryExpr) {
                spacing.add_token_right_expected(syntax_id, TokenSpacingExpected::Space(0));
            } else {
                apply_space_rule(
                    spacing,
                    syntax_id,
                    space_around_binary_op(binary_op_for_plus_minus(token), ctx.config),
                );
            }
        }
        LuaTokenKind::TkMul
        | LuaTokenKind::TkDiv
        | LuaTokenKind::TkIDiv
        | LuaTokenKind::TkMod
        | LuaTokenKind::TkPow
        | LuaTokenKind::TkConcat
        | LuaTokenKind::TkBitAnd
        | LuaTokenKind::TkBitOr
        | LuaTokenKind::TkBitXor
        | LuaTokenKind::TkShl
        | LuaTokenKind::TkShr
        | LuaTokenKind::TkEq
        | LuaTokenKind::TkGe
        | LuaTokenKind::TkGt
        | LuaTokenKind::TkLe
        | LuaTokenKind::TkLt
        | LuaTokenKind::TkNe
        | LuaTokenKind::TkAnd
        | LuaTokenKind::TkOr => apply_operator_spacing(ctx, spacing, token, syntax_id),
        LuaTokenKind::TkDocOr | LuaTokenKind::TkDocAnd => {
            apply_space_rule(spacing, syntax_id, SpaceRule::NoSpace);
        }
        LuaTokenKind::TkDocExtends | LuaTokenKind::TkDocIn => {
            apply_space_rule(spacing, syntax_id, SpaceRule::Space);
        }
        LuaTokenKind::TkAssign => {
            apply_space_rule(spacing, syntax_id, space_around_assign(ctx.config));
        }
        LuaTokenKind::TkLocal
        | LuaTokenKind::TkFunction
        | LuaTokenKind::TkIf
        | LuaTokenKind::TkWhile
        | LuaTokenKind::TkFor
        | LuaTokenKind::TkRepeat
        | LuaTokenKind::TkReturn
        | LuaTokenKind::TkDo
        | LuaTokenKind::TkElseIf
        | LuaTokenKind::TkElse
        | LuaTokenKind::TkThen
        | LuaTokenKind::TkUntil
        | LuaTokenKind::TkIn
        | LuaTokenKind::TkNot => {
            spacing.add_token_left_expected(syntax_id, TokenSpacingExpected::Space(1));
            spacing.add_token_right_expected(syntax_id, TokenSpacingExpected::Space(1));
        }
        _ => {}
    }
}

fn apply_left_paren_spacing(
    ctx: &FormatContext,
    spacing: &mut RootSpacingModel,
    token: &LuaSyntaxToken,
    syntax_id: LuaSyntaxId,
) {
    let left_space = if is_parent_syntax(token, LuaSyntaxKind::ParamList) {
        usize::from(ctx.config.spacing.space_before_func_paren)
    } else if is_parent_syntax(token, LuaSyntaxKind::CallArgList) {
        usize::from(ctx.config.spacing.space_before_call_paren)
    } else if let Some(prev_token) = get_prev_sibling_token_without_space(token) {
        match prev_token.kind().to_token() {
            LuaTokenKind::TkName
            | LuaTokenKind::TkRightParen
            | LuaTokenKind::TkRightBracket
            | LuaTokenKind::TkFunction => 0,
            LuaTokenKind::TkString | LuaTokenKind::TkRightBrace | LuaTokenKind::TkLongString => 1,
            _ => 0,
        }
    } else {
        0
    };

    spacing.add_token_left_expected(syntax_id, TokenSpacingExpected::Space(left_space));
    spacing.add_token_right_expected(
        syntax_id,
        TokenSpacingExpected::Space(space_inside_parens(token, ctx)),
    );
}

fn apply_right_paren_spacing(
    ctx: &FormatContext,
    spacing: &mut RootSpacingModel,
    token: &LuaSyntaxToken,
    syntax_id: LuaSyntaxId,
) {
    spacing.add_token_left_expected(
        syntax_id,
        TokenSpacingExpected::Space(space_inside_parens(token, ctx)),
    );
}

fn apply_left_bracket_spacing(
    ctx: &FormatContext,
    spacing: &mut RootSpacingModel,
    token: &LuaSyntaxToken,
    syntax_id: LuaSyntaxId,
) {
    let left_space = if let Some(prev_token) = get_prev_sibling_token_without_space(token) {
        match prev_token.kind().to_token() {
            LuaTokenKind::TkName
            | LuaTokenKind::TkRightParen
            | LuaTokenKind::TkRightBracket
            | LuaTokenKind::TkDot
            | LuaTokenKind::TkColon => 0,
            LuaTokenKind::TkString | LuaTokenKind::TkRightBrace | LuaTokenKind::TkLongString => 1,
            _ => 0,
        }
    } else {
        0
    };

    spacing.add_token_left_expected(syntax_id, TokenSpacingExpected::Space(left_space));
    spacing.add_token_right_expected(
        syntax_id,
        TokenSpacingExpected::Space(space_inside_brackets(token, ctx)),
    );
}

fn apply_operator_spacing(
    ctx: &FormatContext,
    spacing: &mut RootSpacingModel,
    token: &LuaSyntaxToken,
    syntax_id: LuaSyntaxId,
) {
    match token.kind().to_token() {
        LuaTokenKind::TkLt if in_comment(token) => {
            spacing.add_token_left_expected(syntax_id, TokenSpacingExpected::Space(0));
            spacing.add_token_right_expected(syntax_id, TokenSpacingExpected::Space(0));
        }
        LuaTokenKind::TkGt if in_comment(token) => {
            spacing.add_token_left_expected(syntax_id, TokenSpacingExpected::Space(0));
        }
        LuaTokenKind::TkLt | LuaTokenKind::TkGt
            if is_parent_syntax(token, LuaSyntaxKind::Attribute) =>
        {
            let (left, right) = if token.kind().to_token() == LuaTokenKind::TkLt {
                (1, 0)
            } else {
                (0, 1)
            };
            spacing.add_token_left_expected(syntax_id, TokenSpacingExpected::Space(left));
            spacing.add_token_right_expected(syntax_id, TokenSpacingExpected::Space(right));
        }
        _ => {
            let Some(rule) = binary_space_rule_for_token(ctx, token) else {
                return;
            };
            apply_space_rule(spacing, syntax_id, rule);
        }
    }
}

fn apply_comment_start_spacing(
    ctx: &FormatContext,
    spacing: &mut RootSpacingModel,
    token: &LuaSyntaxToken,
    syntax_id: LuaSyntaxId,
) {
    if !in_comment(token) {
        return;
    }

    if let Some(replacement) = normalized_comment_prefix(ctx, token.text()) {
        spacing.add_token_replace(syntax_id, replacement);
        spacing.add_token_right_expected(syntax_id, TokenSpacingExpected::Space(0));
    }
}

fn binary_space_rule_for_token(ctx: &FormatContext, token: &LuaSyntaxToken) -> Option<SpaceRule> {
    let op = match token.kind().to_token() {
        LuaTokenKind::TkPlus => BinaryOperator::OpAdd,
        LuaTokenKind::TkMinus => BinaryOperator::OpSub,
        LuaTokenKind::TkMul => BinaryOperator::OpMul,
        LuaTokenKind::TkDiv => BinaryOperator::OpDiv,
        LuaTokenKind::TkIDiv => BinaryOperator::OpIDiv,
        LuaTokenKind::TkMod => BinaryOperator::OpMod,
        LuaTokenKind::TkPow => BinaryOperator::OpPow,
        LuaTokenKind::TkConcat => BinaryOperator::OpConcat,
        LuaTokenKind::TkBitAnd => BinaryOperator::OpBAnd,
        LuaTokenKind::TkBitOr => BinaryOperator::OpBOr,
        LuaTokenKind::TkBitXor => BinaryOperator::OpBXor,
        LuaTokenKind::TkShl => BinaryOperator::OpShl,
        LuaTokenKind::TkShr => BinaryOperator::OpShr,
        LuaTokenKind::TkEq => BinaryOperator::OpEq,
        LuaTokenKind::TkGe => BinaryOperator::OpGe,
        LuaTokenKind::TkGt => BinaryOperator::OpGt,
        LuaTokenKind::TkLe => BinaryOperator::OpLe,
        LuaTokenKind::TkLt => BinaryOperator::OpLt,
        LuaTokenKind::TkNe => BinaryOperator::OpNe,
        LuaTokenKind::TkAnd => BinaryOperator::OpAnd,
        LuaTokenKind::TkOr => BinaryOperator::OpOr,
        _ => return None,
    };

    Some(space_around_binary_op(op, ctx.config))
}

fn binary_op_for_plus_minus(token: &LuaSyntaxToken) -> BinaryOperator {
    match token.kind().to_token() {
        LuaTokenKind::TkPlus => BinaryOperator::OpAdd,
        LuaTokenKind::TkMinus => BinaryOperator::OpSub,
        _ => BinaryOperator::OpNop,
    }
}

fn apply_space_rule(spacing: &mut RootSpacingModel, syntax_id: LuaSyntaxId, rule: SpaceRule) {
    match rule {
        SpaceRule::Space => {
            spacing.add_token_left_expected(syntax_id, TokenSpacingExpected::Space(1));
            spacing.add_token_right_expected(syntax_id, TokenSpacingExpected::Space(1));
        }
        SpaceRule::NoSpace => {
            spacing.add_token_left_expected(syntax_id, TokenSpacingExpected::Space(0));
            spacing.add_token_right_expected(syntax_id, TokenSpacingExpected::Space(0));
        }
    }
}

fn space_inside_parens(token: &LuaSyntaxToken, ctx: &FormatContext) -> usize {
    if is_parent_syntax(token, LuaSyntaxKind::ParenExpr) {
        usize::from(ctx.config.spacing.space_inside_parens)
    } else {
        0
    }
}

fn space_inside_brackets(_token: &LuaSyntaxToken, ctx: &FormatContext) -> usize {
    usize::from(ctx.config.spacing.space_inside_brackets)
}

fn space_inside_braces(_token: &LuaSyntaxToken, ctx: &FormatContext) -> usize {
    usize::from(ctx.config.spacing.space_inside_braces)
}

fn is_parent_syntax(token: &LuaSyntaxToken, kind: LuaSyntaxKind) -> bool {
    token
        .parent()
        .is_some_and(|parent| parent.kind().to_syntax() == kind)
}

fn in_comment(token: &LuaSyntaxToken) -> bool {
    let mut current = token.parent();
    while let Some(node) = current {
        if node.kind().to_syntax() == LuaSyntaxKind::Comment {
            return true;
        }
        current = node.parent();
    }

    false
}

fn get_prev_sibling_token_without_space(token: &LuaSyntaxToken) -> Option<LuaSyntaxToken> {
    let mut current = token.clone();
    while let Some(prev) = current.prev_token() {
        if !matches!(
            prev.kind().to_token(),
            LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine
        ) {
            return Some(prev);
        }
        current = prev;
    }

    None
}

fn normalized_comment_prefix(ctx: &FormatContext, prefix_text: &str) -> Option<String> {
    match dash_prefix_len(prefix_text) {
        2 => Some(if ctx.config.comments.space_after_comment_dash {
            "-- ".to_string()
        } else {
            "--".to_string()
        }),
        3 => Some(if ctx.config.emmy_doc.space_after_description_dash {
            "--- ".to_string()
        } else {
            "---".to_string()
        }),
        _ => None,
    }
}

fn normalized_doc_tag_prefix(ctx: &FormatContext) -> String {
    if ctx.config.emmy_doc.space_between_tag_columns {
        "--- @".to_string()
    } else {
        "---@".to_string()
    }
}

fn normalized_doc_continue_prefix(ctx: &FormatContext, prefix_text: &str) -> String {
    if prefix_text == "---" || prefix_text == "--- " {
        if ctx.config.emmy_doc.space_after_description_dash {
            "--- ".to_string()
        } else {
            "---".to_string()
        }
    } else {
        prefix_text.to_string()
    }
}

fn normalized_doc_continue_or_prefix(ctx: &FormatContext, prefix_text: &str) -> String {
    if !prefix_text.starts_with("---") {
        return prefix_text.to_string();
    }

    let suffix = prefix_text[3..].trim_start();
    if ctx.config.emmy_doc.space_after_description_dash {
        format!("--- {suffix}")
    } else {
        format!("---{suffix}")
    }
}

fn dash_prefix_len(prefix_text: &str) -> usize {
    prefix_text.bytes().take_while(|byte| *byte == b'-').count()
}

#[cfg(test)]
mod tests {
    use emmylua_parser::{LuaLanguageLevel, LuaParser, ParserConfig};

    use crate::config::LuaFormatConfig;

    use super::*;

    fn find_token(chunk: &LuaChunk, kind: LuaTokenKind) -> LuaSyntaxToken {
        chunk
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind().to_token() == kind)
            .unwrap()
    }

    #[test]
    fn test_spacing_assign_defaults_to_single_spaces() {
        let config = LuaFormatConfig::default();
        let tree = LuaParser::parse(
            "local x=1\n",
            ParserConfig::with_level(LuaLanguageLevel::Lua54),
        );
        let chunk = tree.get_chunk_node();
        let spacing = analyze_root_spacing(&FormatContext::new(&config), &chunk).spacing;
        let assign = find_token(&chunk, LuaTokenKind::TkAssign);
        let assign_id = LuaSyntaxId::from_token(&assign);

        assert_eq!(
            spacing.left_expected(assign_id),
            Some(&TokenSpacingExpected::Space(1))
        );
        assert_eq!(
            spacing.right_expected(assign_id),
            Some(&TokenSpacingExpected::Space(1))
        );
    }

    #[test]
    fn test_spacing_uses_call_paren_config() {
        let config = LuaFormatConfig {
            spacing: crate::config::SpacingConfig {
                space_before_call_paren: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let tree = LuaParser::parse(
            "foo(a)\n",
            ParserConfig::with_level(LuaLanguageLevel::Lua54),
        );
        let chunk = tree.get_chunk_node();
        let spacing = analyze_root_spacing(&FormatContext::new(&config), &chunk).spacing;
        let left_paren = find_token(&chunk, LuaTokenKind::TkLeftParen);
        let paren_id = LuaSyntaxId::from_token(&left_paren);

        assert_eq!(
            spacing.left_expected(paren_id),
            Some(&TokenSpacingExpected::Space(1))
        );
        assert_eq!(
            spacing.right_expected(paren_id),
            Some(&TokenSpacingExpected::Space(0))
        );
    }

    #[test]
    fn test_spacing_respects_paren_expr_inner_space() {
        let config = LuaFormatConfig {
            spacing: crate::config::SpacingConfig {
                space_inside_parens: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let tree = LuaParser::parse(
            "local x = (a)\n",
            ParserConfig::with_level(LuaLanguageLevel::Lua54),
        );
        let chunk = tree.get_chunk_node();
        let spacing = analyze_root_spacing(&FormatContext::new(&config), &chunk).spacing;
        let left_paren = find_token(&chunk, LuaTokenKind::TkLeftParen);
        let right_paren = find_token(&chunk, LuaTokenKind::TkRightParen);

        assert_eq!(
            spacing.right_expected(LuaSyntaxId::from_token(&left_paren)),
            Some(&TokenSpacingExpected::Space(1))
        );
        assert_eq!(
            spacing.left_expected(LuaSyntaxId::from_token(&right_paren)),
            Some(&TokenSpacingExpected::Space(1))
        );
    }

    #[test]
    fn test_spacing_respects_math_operator_config() {
        let config = LuaFormatConfig {
            spacing: crate::config::SpacingConfig {
                space_around_math_operator: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let tree = LuaParser::parse(
            "local x = a+b\n",
            ParserConfig::with_level(LuaLanguageLevel::Lua54),
        );
        let chunk = tree.get_chunk_node();
        let spacing = analyze_root_spacing(&FormatContext::new(&config), &chunk).spacing;
        let plus = find_token(&chunk, LuaTokenKind::TkPlus);
        let plus_id = LuaSyntaxId::from_token(&plus);

        assert_eq!(
            spacing.left_expected(plus_id),
            Some(&TokenSpacingExpected::Space(0))
        );
        assert_eq!(
            spacing.right_expected(plus_id),
            Some(&TokenSpacingExpected::Space(0))
        );
    }

    #[test]
    fn test_spacing_collects_comment_prefix_replacement() {
        let config = LuaFormatConfig::default();
        let tree = LuaParser::parse(
            "--hello\n",
            ParserConfig::with_level(LuaLanguageLevel::Lua54),
        );
        let chunk = tree.get_chunk_node();
        let spacing = analyze_root_spacing(&FormatContext::new(&config), &chunk).spacing;
        let start = find_token(&chunk, LuaTokenKind::TkNormalStart);
        let start_id = LuaSyntaxId::from_token(&start);

        assert_eq!(spacing.token_replace(start_id), Some("-- "));
        assert_eq!(
            spacing.right_expected(start_id),
            Some(&TokenSpacingExpected::Space(0))
        );
    }

    #[test]
    fn test_spacing_collects_doc_prefix_replacement() {
        let config = LuaFormatConfig::default();
        let tree = LuaParser::parse(
            "---@param x string\n",
            ParserConfig::with_level(LuaLanguageLevel::Lua54),
        );
        let chunk = tree.get_chunk_node();
        let spacing = analyze_root_spacing(&FormatContext::new(&config), &chunk).spacing;
        let start = find_token(&chunk, LuaTokenKind::TkDocStart);

        assert_eq!(
            spacing.token_replace(LuaSyntaxId::from_token(&start)),
            Some("--- @")
        );
    }
}

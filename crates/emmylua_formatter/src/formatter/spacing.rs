use crate::config::LuaFormatConfig;
use crate::ir::{self, DocIR};
use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaChunk, LuaKind, LuaSyntaxId, LuaSyntaxKind, LuaSyntaxToken,
    LuaTokenKind,
};
use smol_str::SmolStr;

use super::FormatContext;
use super::model::{FormatPlan, SpacingModel, TokenSpacingExpected};

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

pub fn analyze_spacing(ctx: &FormatContext, chunk: &LuaChunk, plan: &mut FormatPlan) {
    plan.spacing.has_shebang = chunk
        .syntax()
        .first_token()
        .is_some_and(|token| token.kind() == LuaKind::Token(LuaTokenKind::TkShebang));

    analyze_chunk_token_spacing(ctx, chunk, &mut plan.spacing)
}

fn analyze_chunk_token_spacing(ctx: &FormatContext, chunk: &LuaChunk, spacing: &mut SpacingModel) {
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

fn analyze_token_spacing(ctx: &FormatContext, spacing: &mut SpacingModel, token: &LuaSyntaxToken) {
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
            } else {
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
        LuaTokenKind::TkDocOr => {
            apply_space_rule(spacing, syntax_id, SpaceRule::Space);
        }
        LuaTokenKind::TkDocAnd
        | LuaTokenKind::TkDocExtends
        | LuaTokenKind::TkDocIn
        | LuaTokenKind::TkDocNew => {
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
            apply_space_rule(spacing, syntax_id, SpaceRule::Space);
        }
        LuaTokenKind::TkDocQuestion => {
            spacing.add_token_left_expected(syntax_id, TokenSpacingExpected::Space(0));
            spacing.add_token_right_expected(syntax_id, TokenSpacingExpected::Space(1));
        }
        _ => {}
    }
}

fn apply_left_paren_spacing(
    ctx: &FormatContext,
    spacing: &mut SpacingModel,
    token: &LuaSyntaxToken,
    syntax_id: LuaSyntaxId,
) {
    let Some(prev_token) = get_prev_sibling_token_without_space(token) else {
        return;
    };
    let left_space = if is_parent_syntax(token, LuaSyntaxKind::ParamList)
        && prev_token.kind().to_token() != LuaTokenKind::TkFunction
    {
        Some(if ctx.config.spacing.space_before_func_paren {
            1
        } else {
            0
        })
    } else if is_parent_syntax(token, LuaSyntaxKind::CallArgList) {
        Some(if ctx.config.spacing.space_before_call_paren {
            1
        } else {
            0
        })
    } else {
        match prev_token.kind().to_token() {
            LuaTokenKind::TkName | LuaTokenKind::TkRightParen | LuaTokenKind::TkRightBracket => {
                Some(0)
            }
            LuaTokenKind::TkString | LuaTokenKind::TkRightBrace | LuaTokenKind::TkLongString => {
                Some(1)
            }
            LuaTokenKind::TkFunction => {
                if ctx.config.spacing.space_before_lambda_func_paren {
                    Some(1)
                } else {
                    Some(0)
                }
            }
            _ => None,
        }
    };

    if let Some(left_space) = left_space {
        spacing.add_token_left_expected(syntax_id, TokenSpacingExpected::Space(left_space));
    }
    spacing.add_token_right_expected(
        syntax_id,
        TokenSpacingExpected::Space(space_inside_parens(token, ctx)),
    );
}

fn apply_right_paren_spacing(
    ctx: &FormatContext,
    spacing: &mut SpacingModel,
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
    spacing: &mut SpacingModel,
    token: &LuaSyntaxToken,
    syntax_id: LuaSyntaxId,
) {
    let left_space = if let Some(prev_token) = get_prev_sibling_token_without_space(token) {
        match prev_token.kind().to_token() {
            LuaTokenKind::TkComma | LuaTokenKind::TkSemicolon => Some(1),
            LuaTokenKind::TkName
            | LuaTokenKind::TkRightParen
            | LuaTokenKind::TkRightBracket
            | LuaTokenKind::TkDot
            | LuaTokenKind::TkColon => Some(0),
            LuaTokenKind::TkString
            | LuaTokenKind::TkRightBrace
            | LuaTokenKind::TkLongString
            | LuaTokenKind::TkLeftBrace => Some(1),
            _ => None,
        }
    } else {
        None
    };

    if let Some(left_space) = left_space {
        spacing.add_token_left_expected(syntax_id, TokenSpacingExpected::Space(left_space));
    }
    spacing.add_token_right_expected(
        syntax_id,
        TokenSpacingExpected::Space(space_inside_brackets(token, ctx)),
    );
}

fn apply_operator_spacing(
    ctx: &FormatContext,
    spacing: &mut SpacingModel,
    token: &LuaSyntaxToken,
    syntax_id: LuaSyntaxId,
) {
    match token.kind().to_token() {
        LuaTokenKind::TkLt if is_generic_angle_bracket(token) => {
            spacing.add_token_left_expected(syntax_id, TokenSpacingExpected::Space(0));
            spacing.add_token_right_expected(syntax_id, TokenSpacingExpected::Space(0));
        }
        LuaTokenKind::TkGt if is_generic_angle_bracket(token) => {
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
        LuaTokenKind::TkLt | LuaTokenKind::TkGt
            if is_parent_syntax(token, LuaSyntaxKind::DocVersion) =>
        {
            spacing.add_token_right_expected(syntax_id, TokenSpacingExpected::Space(0));
        }
        _ => {
            let Some(rule) = binary_space_rule_for_token(ctx, token) else {
                return;
            };
            apply_space_rule(spacing, syntax_id, rule);
        }
    }
}

fn is_generic_angle_bracket(token: &LuaSyntaxToken) -> bool {
    let token_kind = token.kind().to_token();
    if !matches!(token_kind, LuaTokenKind::TkLt | LuaTokenKind::TkGt) {
        return false;
    }
    token.parent().is_some_and(|parent| {
        matches!(
            parent.kind().to_syntax(),
            LuaSyntaxKind::DocGenericDeclareList | LuaSyntaxKind::TypeGeneric
        )
    })
}

fn apply_comment_start_spacing(
    ctx: &FormatContext,
    spacing: &mut SpacingModel,
    token: &LuaSyntaxToken,
    syntax_id: LuaSyntaxId,
) {
    // bug fix: -- [[
    if comment_start_looks_like_spaced_long_comment(token) {
        return;
    }

    if let Some(replacement) = normalized_comment_prefix(ctx, token.text()) {
        spacing.add_token_replace(syntax_id, replacement);
        spacing.add_token_right_expected(syntax_id, TokenSpacingExpected::Space(0));
    }
}

fn comment_start_looks_like_spaced_long_comment(token: &LuaSyntaxToken) -> bool {
    if token.kind().to_token() != LuaTokenKind::TkNormalStart || token.text() != "--" {
        return false;
    }

    let mut current = token.next_token();
    let mut saw_whitespace = false;
    while let Some(next) = current {
        match next.kind().to_token() {
            LuaTokenKind::TkWhitespace => {
                saw_whitespace = true;
                current = next.next_token();
            }
            LuaTokenKind::TkEndOfLine => return false,
            _ => return saw_whitespace && next.text().starts_with('['),
        }
    }

    false
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

fn apply_space_rule(spacing: &mut SpacingModel, syntax_id: LuaSyntaxId, rule: SpaceRule) {
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
    if ctx.config.spacing.space_inside_brackets {
        1
    } else {
        0
    }
}

fn space_inside_braces(_token: &LuaSyntaxToken, ctx: &FormatContext) -> usize {
    if ctx.config.spacing.space_inside_braces {
        1
    } else {
        0
    }
}

fn is_parent_syntax(token: &LuaSyntaxToken, kind: LuaSyntaxKind) -> bool {
    token
        .parent()
        .is_some_and(|parent| parent.kind().to_syntax() == kind)
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

fn normalized_comment_prefix(ctx: &FormatContext, prefix_text: &str) -> Option<SmolStr> {
    match dash_prefix_len(prefix_text) {
        2 => Some(if ctx.config.comments.space_after_comment_dash {
            "-- ".into()
        } else {
            "--".into()
        }),
        3 => Some(if ctx.config.emmy_doc.space_after_description_dash {
            "--- ".into()
        } else {
            "---".into()
        }),
        _ => None,
    }
}

fn normalized_doc_tag_prefix(ctx: &FormatContext) -> SmolStr {
    if ctx.config.emmy_doc.space_between_tag_columns {
        "--- @".into()
    } else {
        "---@".into()
    }
}

fn normalized_doc_continue_prefix(ctx: &FormatContext, prefix_text: &str) -> SmolStr {
    if prefix_text == "---" || prefix_text == "--- " {
        if ctx.config.emmy_doc.space_after_description_dash {
            "--- ".into()
        } else {
            "---".into()
        }
    } else {
        prefix_text.into()
    }
}

fn normalized_doc_continue_or_prefix(ctx: &FormatContext, prefix_text: &str) -> SmolStr {
    if !prefix_text.starts_with("---") {
        return prefix_text.into();
    }

    let suffix = prefix_text[3..].trim_start();
    if ctx.config.emmy_doc.space_after_description_dash {
        format!("--- {suffix}").into()
    } else {
        format!("---{suffix}").into()
    }
}

fn dash_prefix_len(prefix_text: &str) -> usize {
    prefix_text.bytes().take_while(|byte| *byte == b'-').count()
}

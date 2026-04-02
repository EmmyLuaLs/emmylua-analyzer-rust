use emmylua_parser::{
    LexerState, LuaAstNode, LuaAstToken, LuaComment, LuaLiteralExpr, LuaLiteralToken,
    LuaStringToken, Reader, SourceRange,
};
use emmylua_parser_desc::{
    CodeBlockHighlightKind, CodeBlockLang, DescItem, DescItemKind, ResultContainer, process_code,
};
use rowan::{TextRange, TextSize};

use crate::handlers::semantic_token::semantic_token_builder::{
    SemanticBuilder, SemanticTokenTypeKind,
};

pub fn inject_language(
    builder: &mut SemanticBuilder,
    lang_name: &str,
    comment: LuaComment,
) -> Option<()> {
    // Implementation for injecting the language
    let owner = comment.get_owner()?;
    let lang = CodeBlockLang::try_parse(lang_name)?;
    for literal in owner.descendants::<LuaLiteralExpr>() {
        if let Some(LuaLiteralToken::String(str_token)) = literal.get_literal() {
            process_inject_lang_string_token(builder, lang, &str_token);
        }
    }

    Some(())
}

pub fn process_inject_lang_string_token(
    builder: &mut SemanticBuilder,
    lang: CodeBlockLang,
    str_token: &LuaStringToken,
) -> Option<()> {
    let code_block_info = divide_into_quote_and_code_block(str_token)?;
    let code_block_range = code_block_info.code_block;
    let code_block_source = SourceRange::from_start_end(
        u32::from(code_block_range.start()) as usize,
        u32::from(code_block_range.end()) as usize,
    );
    let code_block_str = str_token.slice(code_block_range)?;
    let reader = Reader::new(code_block_str);
    let mut result = InjectResult::new();
    process_code(
        &mut result,
        code_block_source,
        reader,
        LexerState::Normal,
        lang,
    );

    for desc_item in result.results() {
        if let DescItemKind::CodeBlockHl(highlight_kind) = desc_item.kind {
            let token_type = match highlight_kind {
                CodeBlockHighlightKind::Keyword => SemanticTokenTypeKind::Keyword,
                CodeBlockHighlightKind::String => SemanticTokenTypeKind::String,
                CodeBlockHighlightKind::Number => SemanticTokenTypeKind::Number,
                CodeBlockHighlightKind::Comment => SemanticTokenTypeKind::Comment,
                CodeBlockHighlightKind::Function => SemanticTokenTypeKind::Function,
                CodeBlockHighlightKind::Class => SemanticTokenTypeKind::Class,
                CodeBlockHighlightKind::Enum => SemanticTokenTypeKind::Enum,
                CodeBlockHighlightKind::Variable => SemanticTokenTypeKind::Variable,
                CodeBlockHighlightKind::Property => SemanticTokenTypeKind::Property,
                CodeBlockHighlightKind::Decorator => SemanticTokenTypeKind::Decorator,
                CodeBlockHighlightKind::Operators => SemanticTokenTypeKind::Operator,
                _ => continue, // Fallback for other kinds
            };

            let sub_token_range = TextRange::new(
                desc_item.range.start() + code_block_range.start(),
                desc_item.range.end() + code_block_range.start(),
            );
            builder.push_at_range(sub_token_range, token_type, None);
        }
    }

    for quote_range in code_block_info.quote_ranges {
        let len = u32::from(quote_range.end() - quote_range.start());
        builder.push_at_position(
            quote_range.start(),
            len,
            SemanticTokenTypeKind::String,
            None,
        );
    }
    builder.add_special_string_range(str_token.get_range());

    Some(())
}

struct InjectResult {
    result: Vec<DescItem>,
}

impl ResultContainer for InjectResult {
    fn results(&self) -> &Vec<DescItem> {
        &self.result
    }

    fn results_mut(&mut self) -> &mut Vec<DescItem> {
        &mut self.result
    }

    fn cursor_position(&self) -> Option<usize> {
        None
    }
}

impl InjectResult {
    pub fn new() -> Self {
        InjectResult { result: Vec::new() }
    }
}

struct CodeBlockInfo {
    code_block: TextRange,
    quote_ranges: Vec<TextRange>,
}

fn divide_into_quote_and_code_block(str_token: &LuaStringToken) -> Option<CodeBlockInfo> {
    let str_token_range = str_token.get_range();
    let text = str_token.get_text();
    let mut quote_ranges = Vec::new();

    if text.is_empty() {
        return None;
    }

    let mut code_block_start = str_token_range.start();
    let mut code_block_end = str_token_range.end();
    let range_start = str_token_range.start();
    if text.starts_with("[[") || text.starts_with("[=") {
        let mut equal_count = 0;
        let mut start_end = 2;

        for c in text.chars().skip(1) {
            if c == '=' {
                equal_count += 1;
            } else if c == '[' {
                start_end = 2 + equal_count;
                break;
            } else {
                break;
            }
        }

        let start_quote_range =
            TextRange::new(range_start, range_start + TextSize::from(start_end as u32));
        quote_ranges.push(start_quote_range);
        code_block_start = start_quote_range.end();

        let end_pattern = format!("]{}]", "=".repeat(equal_count));
        if let Some(end_pos) = text.rfind(&end_pattern) {
            let end_quote_start = range_start + rowan::TextSize::from(end_pos as u32);
            let end_quote_range = TextRange::new(
                end_quote_start,
                range_start + rowan::TextSize::from(text.len() as u32),
            );
            quote_ranges.push(end_quote_range);
            code_block_end = end_quote_range.start();
        }
    } else if text.starts_with('"') || text.starts_with('\'') {
        let quote_char = text.chars().next().unwrap();
        let start_quote_range = TextRange::new(range_start, range_start + TextSize::from(1));
        quote_ranges.push(start_quote_range);
        code_block_start = start_quote_range.end();

        if text.len() > 1 && text.ends_with(quote_char) {
            let end_quote_start = range_start + TextSize::from((text.len() - 1) as u32);
            let end_quote_range = TextRange::new(
                end_quote_start,
                range_start + TextSize::from(text.len() as u32),
            );
            quote_ranges.push(end_quote_range);
            code_block_end = end_quote_range.start();
        }
    }

    if code_block_start > code_block_end {
        return None;
    }

    Some(CodeBlockInfo {
        code_block: TextRange::new(code_block_start, code_block_end),
        quote_ranges,
    })
}

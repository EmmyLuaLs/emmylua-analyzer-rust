use emmylua_code_analysis::LuaDocument;
use emmylua_parser::LuaSyntaxToken;
use lsp_types::{SemanticToken, SemanticTokenModifier, SemanticTokenType};
use rowan::TextSize;
use std::{collections::HashMap, vec::Vec};

pub const SEMANTIC_TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::NAMESPACE,
    SemanticTokenType::TYPE,
    SemanticTokenType::CLASS,
    SemanticTokenType::ENUM,
    SemanticTokenType::INTERFACE,
    SemanticTokenType::STRUCT,
    SemanticTokenType::TYPE_PARAMETER,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::ENUM_MEMBER,
    SemanticTokenType::EVENT,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::METHOD,
    SemanticTokenType::MACRO,
    SemanticTokenType::KEYWORD,
    SemanticTokenType::MODIFIER,
    SemanticTokenType::COMMENT,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::REGEXP,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::DECORATOR,
];

pub const SEMANTIC_TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,
    SemanticTokenModifier::DEFINITION,
    SemanticTokenModifier::READONLY,
    SemanticTokenModifier::STATIC,
    SemanticTokenModifier::ABSTRACT,
    SemanticTokenModifier::DEPRECATED,
    SemanticTokenModifier::ASYNC,
    SemanticTokenModifier::MODIFICATION,
    SemanticTokenModifier::DOCUMENTATION,
    SemanticTokenModifier::DEFAULT_LIBRARY,
];

#[derive(Debug)]
struct BasicSemanticTokenData {
    line: u32,
    col: u32,
    length: u32,
    typ: u32,
    modifiers: u32,
}

#[derive(Debug)]
enum SemanticTokenData {
    Basic(BasicSemanticTokenData),
    MultiLine(Vec<BasicSemanticTokenData>),
}

#[derive(Debug)]
pub struct SemanticBuilder<'a> {
    document: &'a LuaDocument<'a>,
    multi_line_support: bool,
    type_to_id: HashMap<SemanticTokenType, u32>,
    modifier_to_id: HashMap<SemanticTokenModifier, u32>,
    data: HashMap<TextSize, SemanticTokenData>,
}

impl<'a> SemanticBuilder<'a> {
    pub fn new(
        document: &'a LuaDocument,
        multi_line_support: bool,
        types: Vec<SemanticTokenType>,
        modifier: Vec<SemanticTokenModifier>,
    ) -> Self {
        let mut type_to_id = HashMap::new();
        for (i, ty) in types.into_iter().enumerate() {
            type_to_id.insert(ty, i as u32);
        }
        let mut modifier_to_id = HashMap::new();
        for (i, modifier) in modifier.into_iter().enumerate() {
            modifier_to_id.insert(modifier, i as u32);
        }

        Self {
            document,
            multi_line_support,
            type_to_id,
            modifier_to_id,
            data: HashMap::new(),
        }
    }

    fn push_data(&mut self, token: &LuaSyntaxToken, typ: u32, modifiers: u32) -> Option<()> {
        let position = token.text_range().start();
        if self.data.contains_key(&position) {
            return Some(());
        }

        let range = token.text_range();
        let lsp_range = self.document.to_lsp_range(range)?;
        let start_line = lsp_range.start.line;
        let start_col = lsp_range.start.character;
        let end_line = lsp_range.end.line;

        if !self.multi_line_support && start_line != end_line {
            let mut muliti_line_data = vec![];
            muliti_line_data.push(BasicSemanticTokenData {
                line: start_line,
                col: start_col,
                length: 9999,
                typ,
                modifiers,
            });

            for i in start_line + 1..end_line - 1 {
                muliti_line_data.push(BasicSemanticTokenData {
                    line: i,
                    col: 0,
                    length: 9999,
                    typ,
                    modifiers,
                });
            }

            muliti_line_data.push(BasicSemanticTokenData {
                line: end_line,
                col: 0,
                length: lsp_range.end.character,
                typ,
                modifiers,
            });

            self.data
                .insert(position, SemanticTokenData::MultiLine(muliti_line_data));
        } else {
            let length = token.text().chars().count() as u32;
            self.data.insert(
                position,
                SemanticTokenData::Basic(BasicSemanticTokenData {
                    line: start_line as u32,
                    col: start_col as u32,
                    length,
                    typ,
                    modifiers,
                }),
            );
        }

        Some(())
    }

    pub fn push(&mut self, token: &LuaSyntaxToken, ty: SemanticTokenType) -> Option<()> {
        self.push_data(token, *self.type_to_id.get(&ty)?, 0);
        Some(())
    }

    pub fn push_with_modifier(
        &mut self,
        token: &LuaSyntaxToken,
        ty: SemanticTokenType,
        modifier: SemanticTokenModifier,
    ) -> Option<()> {
        let typ = *self.type_to_id.get(&ty)?;
        let modifier = 1 << *self.modifier_to_id.get(&modifier)?;
        self.push_data(token, typ, modifier);
        Some(())
    }

    pub fn push_at_position(
        &mut self,
        position: TextSize,
        length: u32,
        ty: SemanticTokenType,
        modifiers: Option<SemanticTokenModifier>,
    ) -> Option<()> {
        let lsp_position = self.document.to_lsp_position(position)?;
        let start_line = lsp_position.line;
        let start_col = lsp_position.character;

        self.data.insert(
            position,
            SemanticTokenData::Basic(BasicSemanticTokenData {
                line: start_line as u32,
                col: start_col as u32,
                length,
                typ: *self.type_to_id.get(&ty)?,
                modifiers: modifiers.map_or(0, |m| 1 << *self.modifier_to_id.get(&m).unwrap_or(&0)),
            }),
        );
        Some(())
    }

    #[allow(unused)]
    pub fn push_with_modifiers(
        &mut self,
        token: &LuaSyntaxToken,
        ty: SemanticTokenType,
        modifiers: Vec<SemanticTokenModifier>,
    ) -> Option<()> {
        let typ = *self.type_to_id.get(&ty)?;
        let mut modifier = 0;
        for m in modifiers {
            modifier |= 1 << *self.modifier_to_id.get(&m)?;
        }
        self.push_data(token, typ, modifier);

        Some(())
    }

    pub fn build(self) -> Vec<SemanticToken> {
        let mut data: Vec<BasicSemanticTokenData> = vec![];
        for (_, token_data) in self.data {
            match token_data {
                SemanticTokenData::Basic(basic_data) => {
                    data.push(basic_data);
                }
                SemanticTokenData::MultiLine(multi_data) => {
                    for basic_data in multi_data {
                        data.push(basic_data);
                    }
                }
            }
        }

        data.sort_by(|a, b| {
            let line1 = a.line;
            let line2 = b.line;
            if line1 == line2 {
                let character1 = a.col;
                let character2 = b.col;
                return character1.cmp(&character2);
            }
            line1.cmp(&line2)
        });

        let mut result = Vec::with_capacity(data.len());
        let mut prev_line = 0;
        let mut prev_col = 0;

        for token_data in data {
            let line_diff = token_data.line - prev_line;
            if line_diff != 0 {
                prev_col = 0;
            }
            let col_diff = token_data.col - prev_col;

            result.push(SemanticToken {
                delta_line: line_diff,
                delta_start: col_diff,
                length: token_data.length,
                token_type: token_data.typ,
                token_modifiers_bitset: token_data.modifiers,
            });

            prev_line = token_data.line;
            prev_col = token_data.col;
        }

        result
    }
}

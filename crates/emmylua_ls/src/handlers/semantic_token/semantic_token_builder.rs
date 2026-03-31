use emmylua_code_analysis::LuaDocument;
use emmylua_parser::LuaSyntaxToken;
use lsp_types::{SemanticToken, SemanticTokenModifier, SemanticTokenType};
use rowan::{TextRange, TextSize};
use std::{
    collections::HashSet,
    ops::{BitOr, BitOrAssign},
    vec::Vec,
};

pub struct CustomSemanticTokenType;
impl CustomSemanticTokenType {
    // neovim supports custom semantic token types, we add a custom type for delimiter
    pub const DELIMITER: SemanticTokenType = SemanticTokenType::new("delimiter");
}

#[allow(unused)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticTokenTypeKind {
    Namespace,
    Type,
    Class,
    Enum,
    Interface,
    Struct,
    TypeParameter,
    Parameter,
    Variable,
    Property,
    EnumMember,
    Event,
    Function,
    Method,
    Macro,
    Keyword,
    Modifier,
    Comment,
    String,
    Number,
    Regexp,
    Operator,
    Decorator,

    // Custom types
    Delimiter,
}

impl SemanticTokenTypeKind {
    #[allow(unused)]
    pub fn to_semantic_token_type(&self) -> SemanticTokenType {
        match self {
            SemanticTokenTypeKind::Namespace => SemanticTokenType::NAMESPACE,
            SemanticTokenTypeKind::Type => SemanticTokenType::TYPE,
            SemanticTokenTypeKind::Class => SemanticTokenType::CLASS,
            SemanticTokenTypeKind::Enum => SemanticTokenType::ENUM,
            SemanticTokenTypeKind::Interface => SemanticTokenType::INTERFACE,
            SemanticTokenTypeKind::Struct => SemanticTokenType::STRUCT,
            SemanticTokenTypeKind::TypeParameter => SemanticTokenType::TYPE_PARAMETER,
            SemanticTokenTypeKind::Parameter => SemanticTokenType::PARAMETER,
            SemanticTokenTypeKind::Variable => SemanticTokenType::VARIABLE,
            SemanticTokenTypeKind::Property => SemanticTokenType::PROPERTY,
            SemanticTokenTypeKind::EnumMember => SemanticTokenType::ENUM_MEMBER,
            SemanticTokenTypeKind::Event => SemanticTokenType::EVENT,
            SemanticTokenTypeKind::Function => SemanticTokenType::FUNCTION,
            SemanticTokenTypeKind::Method => SemanticTokenType::METHOD,
            SemanticTokenTypeKind::Macro => SemanticTokenType::MACRO,
            SemanticTokenTypeKind::Keyword => SemanticTokenType::KEYWORD,
            SemanticTokenTypeKind::Modifier => SemanticTokenType::MODIFIER,
            SemanticTokenTypeKind::Comment => SemanticTokenType::COMMENT,
            SemanticTokenTypeKind::String => SemanticTokenType::STRING,
            SemanticTokenTypeKind::Number => SemanticTokenType::NUMBER,
            SemanticTokenTypeKind::Regexp => SemanticTokenType::REGEXP,
            SemanticTokenTypeKind::Operator => SemanticTokenType::OPERATOR,
            SemanticTokenTypeKind::Decorator => SemanticTokenType::DECORATOR,

            // Custom types
            SemanticTokenTypeKind::Delimiter => CustomSemanticTokenType::DELIMITER,
        }
    }

    pub fn to_u32(&self) -> u32 {
        match self {
            SemanticTokenTypeKind::Namespace => 0,
            SemanticTokenTypeKind::Type => 1,
            SemanticTokenTypeKind::Class => 2,
            SemanticTokenTypeKind::Enum => 3,
            SemanticTokenTypeKind::Interface => 4,
            SemanticTokenTypeKind::Struct => 5,
            SemanticTokenTypeKind::TypeParameter => 6,
            SemanticTokenTypeKind::Parameter => 7,
            SemanticTokenTypeKind::Variable => 8,
            SemanticTokenTypeKind::Property => 9,
            SemanticTokenTypeKind::EnumMember => 10,
            SemanticTokenTypeKind::Event => 11,
            SemanticTokenTypeKind::Function => 12,
            SemanticTokenTypeKind::Method => 13,
            SemanticTokenTypeKind::Macro => 14,
            SemanticTokenTypeKind::Keyword => 15,
            SemanticTokenTypeKind::Modifier => 16,
            SemanticTokenTypeKind::Comment => 17,
            SemanticTokenTypeKind::String => 18,
            SemanticTokenTypeKind::Number => 19,
            SemanticTokenTypeKind::Regexp => 20,
            SemanticTokenTypeKind::Operator => 21,
            SemanticTokenTypeKind::Decorator => 22,

            // Custom types
            SemanticTokenTypeKind::Delimiter => 23,
        }
    }

    pub fn all_types() -> Vec<SemanticTokenType> {
        vec![
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
            // Custom types
            CustomSemanticTokenType::DELIMITER,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SemanticTokenModifierKind(u32);

impl SemanticTokenModifierKind {
    pub const DECLARATION: SemanticTokenModifierKind = SemanticTokenModifierKind(1 << 0);
    pub const DEFINITION: SemanticTokenModifierKind = SemanticTokenModifierKind(1 << 1);
    pub const READONLY: SemanticTokenModifierKind = SemanticTokenModifierKind(1 << 2);
    pub const STATIC: SemanticTokenModifierKind = SemanticTokenModifierKind(1 << 3);
    pub const ABSTRACT: SemanticTokenModifierKind = SemanticTokenModifierKind(1 << 4);
    pub const DEPRECATED: SemanticTokenModifierKind = SemanticTokenModifierKind(1 << 5);
    pub const ASYNC: SemanticTokenModifierKind = SemanticTokenModifierKind(1 << 6);
    pub const MODIFICATION: SemanticTokenModifierKind = SemanticTokenModifierKind(1 << 7);
    pub const DOCUMENTATION: SemanticTokenModifierKind = SemanticTokenModifierKind(1 << 8);
    pub const DEFAULT_LIBRARY: SemanticTokenModifierKind = SemanticTokenModifierKind(1 << 9);

    pub const fn empty() -> Self {
        SemanticTokenModifierKind(0)
    }

    fn to_modifier(self) -> SemanticTokenModifier {
        match self {
            Self::DECLARATION => SemanticTokenModifier::DECLARATION,
            Self::DEFINITION => SemanticTokenModifier::DEFINITION,
            Self::READONLY => SemanticTokenModifier::READONLY,
            Self::STATIC => SemanticTokenModifier::STATIC,
            Self::ABSTRACT => SemanticTokenModifier::ABSTRACT,
            Self::DEPRECATED => SemanticTokenModifier::DEPRECATED,
            Self::ASYNC => SemanticTokenModifier::ASYNC,
            Self::MODIFICATION => SemanticTokenModifier::MODIFICATION,
            Self::DOCUMENTATION => SemanticTokenModifier::DOCUMENTATION,
            Self::DEFAULT_LIBRARY => SemanticTokenModifier::DEFAULT_LIBRARY,
            _ => unreachable!("Invalid modifier bit"),
        }
    }

    pub fn to_u32(self) -> u32 {
        self.0
    }

    pub fn all_modifiers() -> Vec<SemanticTokenModifier> {
        vec![
            Self::DECLARATION,
            Self::DEFINITION,
            Self::READONLY,
            Self::STATIC,
            Self::ABSTRACT,
            Self::DEPRECATED,
            Self::ASYNC,
            Self::MODIFICATION,
            Self::DOCUMENTATION,
            Self::DEFAULT_LIBRARY,
        ]
        .into_iter()
        .map(|m| m.to_modifier())
        .collect()
    }
}

impl BitOr for SemanticTokenModifierKind {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        SemanticTokenModifierKind(self.0 | rhs.0)
    }
}

impl BitOrAssign for SemanticTokenModifierKind {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

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
    data: Vec<SemanticTokenData>,
    seen_positions: HashSet<TextSize>,
    string_special_range: HashSet<TextRange>,
}

impl<'a> SemanticBuilder<'a> {
    pub fn new(document: &'a LuaDocument, multi_line_support: bool) -> Self {
        Self {
            document,
            multi_line_support,
            data: Vec::new(),
            seen_positions: HashSet::new(),
            string_special_range: HashSet::new(),
        }
    }

    fn push_data(&mut self, range: TextRange, typ: u32, modifiers: u32) {
        let position = range.start();
        if !self.seen_positions.insert(position) {
            return;
        }

        let (start_line, start_col) = match self.document.get_line_col(range.start()) {
            Some(pos) => pos,
            None => return,
        };
        let (end_line, end_col) = match self.document.get_line_col(range.end()) {
            Some(pos) => pos,
            None => return,
        };
        let start_line = start_line as u32;
        let start_col = start_col as u32;
        let end_line = end_line as u32;
        let end_col = end_col as u32;

        if !self.multi_line_support && start_line != end_line {
            let mut multi_line_data = vec![];
            multi_line_data.push(BasicSemanticTokenData {
                line: start_line,
                col: start_col,
                length: 9999,
                typ,
                modifiers,
            });

            for i in start_line + 1..end_line {
                multi_line_data.push(BasicSemanticTokenData {
                    line: i,
                    col: 0,
                    length: 9999,
                    typ,
                    modifiers,
                });
            }

            multi_line_data.push(BasicSemanticTokenData {
                line: end_line,
                col: 0,
                length: end_col,
                typ,
                modifiers,
            });

            self.data
                .push(SemanticTokenData::MultiLine(multi_line_data));
        } else {
            self.data
                .push(SemanticTokenData::Basic(BasicSemanticTokenData {
                    line: start_line,
                    col: start_col,
                    length: end_col.saturating_sub(start_col),
                    typ,
                    modifiers,
                }));
        }
    }

    pub fn push(&mut self, token: &LuaSyntaxToken, ty: SemanticTokenTypeKind) {
        self.push_data(token.text_range(), ty.to_u32(), 0);
    }

    pub fn push_with_modifier(
        &mut self,
        token: &LuaSyntaxToken,
        ty: SemanticTokenTypeKind,
        modifier: SemanticTokenModifierKind,
    ) {
        self.push_data(token.text_range(), ty.to_u32(), modifier.to_u32());
    }

    pub fn push_at_position(
        &mut self,
        position: TextSize,
        length: u32,
        ty: SemanticTokenTypeKind,
        modifiers: Option<SemanticTokenModifierKind>,
    ) {
        if !self.seen_positions.insert(position) {
            return;
        }

        let lsp_position = match self.document.to_lsp_position(position) {
            Some(pos) => pos,
            None => return,
        };
        let start_line = lsp_position.line;
        let start_col = lsp_position.character;

        self.data
            .push(SemanticTokenData::Basic(BasicSemanticTokenData {
                line: start_line,
                col: start_col,
                length,
                typ: ty.to_u32(),
                modifiers: modifiers.as_ref().map(|m| m.to_u32()).unwrap_or(0),
            }));
    }

    pub fn push_at_range(
        &mut self,
        range: TextRange,
        ty: SemanticTokenTypeKind,
        modifiers: Option<SemanticTokenModifierKind>,
    ) {
        self.push_data(
            range,
            ty.to_u32(),
            modifiers.map(|m| m.to_u32()).unwrap_or(0),
        );
    }

    pub fn build(self) -> Vec<SemanticToken> {
        let mut data: Vec<BasicSemanticTokenData> = vec![];
        for token_data in self.data {
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

        data.sort_unstable_by(|a, b| {
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

    pub fn add_special_string_range(&mut self, range: TextRange) {
        self.string_special_range.insert(range);
    }

    pub fn is_special_string_range(&self, range: &TextRange) -> bool {
        self.string_special_range.contains(range)
    }

    pub fn contains_position(&self, position: TextSize) -> bool {
        self.seen_positions.contains(&position)
    }

    pub fn contains_token(&self, token: &LuaSyntaxToken) -> bool {
        self.contains_position(token.text_range().start())
    }
}

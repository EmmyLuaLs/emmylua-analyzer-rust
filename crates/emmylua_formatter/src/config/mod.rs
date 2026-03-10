use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LuaFormatConfig {
    // ===== Indentation =====
    pub indent_style: IndentStyle,
    pub tab_width: usize,

    // ===== Line width =====
    pub max_line_width: usize,

    // ===== Blank lines =====
    pub max_blank_lines: usize,

    // ===== Trailing =====
    pub insert_final_newline: bool,
    pub trailing_comma: TrailingComma,

    // ===== Spacing =====
    pub space_before_call_paren: bool,
    pub space_before_func_paren: bool,
    pub space_inside_braces: bool,
    pub space_inside_parens: bool,
    pub space_inside_brackets: bool,

    // ===== End of line =====
    pub end_of_line: EndOfLine,

    // ===== Line break style =====
    pub table_expand: ExpandStrategy,
    pub call_args_expand: ExpandStrategy,
    pub func_params_expand: ExpandStrategy,

    // ===== Alignment =====
    /// Align trailing comments on consecutive lines
    pub align_continuous_line_comment: bool,
    /// Align `=` signs in consecutive assignment statements
    pub align_continuous_assign_statement: bool,
    /// Align `=` signs in table fields
    pub align_table_field: bool,
}

impl Default for LuaFormatConfig {
    fn default() -> Self {
        Self {
            indent_style: IndentStyle::Space(4),
            tab_width: 4,
            max_line_width: 120,
            max_blank_lines: 1,
            insert_final_newline: true,
            trailing_comma: TrailingComma::Never,
            space_before_call_paren: false,
            space_before_func_paren: false,
            space_inside_braces: true,
            space_inside_parens: false,
            space_inside_brackets: false,
            table_expand: ExpandStrategy::Auto,
            call_args_expand: ExpandStrategy::Auto,
            func_params_expand: ExpandStrategy::Auto,
            end_of_line: EndOfLine::LF,
            align_continuous_line_comment: true,
            align_continuous_assign_statement: true,
            align_table_field: true,
        }
    }
}

impl LuaFormatConfig {
    pub fn indent_width(&self) -> usize {
        match &self.indent_style {
            IndentStyle::Tab => self.tab_width,
            IndentStyle::Space(n) => *n,
        }
    }

    pub fn indent_str(&self) -> String {
        match &self.indent_style {
            IndentStyle::Tab => "\t".to_string(),
            IndentStyle::Space(n) => " ".repeat(*n),
        }
    }

    pub fn newline_str(&self) -> &'static str {
        match &self.end_of_line {
            EndOfLine::LF => "\n",
            EndOfLine::CRLF => "\r\n",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IndentStyle {
    Tab,
    Space(usize),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrailingComma {
    Never,
    Multiline,
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExpandStrategy {
    Never,
    Always,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EndOfLine {
    LF,
    CRLF,
}

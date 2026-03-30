use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LuaFormatConfig {
    pub indent: IndentConfig,
    pub layout: LayoutConfig,
    pub output: OutputConfig,
    pub spacing: SpacingConfig,
    pub comments: CommentConfig,
    pub emmy_doc: EmmyDocConfig,
    pub align: AlignConfig,
}

impl LuaFormatConfig {
    pub fn indent_width(&self) -> usize {
        self.indent.width
    }

    pub fn indent_str(&self) -> String {
        match &self.indent.kind {
            IndentKind::Tab => "\t".to_string(),
            IndentKind::Space => " ".repeat(self.indent.width),
        }
    }

    pub fn newline_str(&self) -> &'static str {
        match &self.output.end_of_line {
            EndOfLine::LF => "\n",
            EndOfLine::CRLF => "\r\n",
        }
    }

    pub fn should_align_statement_line_comments(&self) -> bool {
        self.comments.align_line_comments && self.comments.align_in_statements
    }

    pub fn should_align_table_line_comments(&self) -> bool {
        self.comments.align_line_comments && self.comments.align_in_table_fields
    }

    pub fn should_align_call_arg_line_comments(&self) -> bool {
        self.comments.align_line_comments && self.comments.align_in_call_args
    }

    pub fn should_align_param_line_comments(&self) -> bool {
        self.comments.align_line_comments && self.comments.align_in_params
    }

    pub fn should_align_emmy_doc_declaration_tags(&self) -> bool {
        self.emmy_doc.align_tag_columns && self.emmy_doc.align_declaration_tags
    }

    pub fn should_align_emmy_doc_reference_tags(&self) -> bool {
        self.emmy_doc.align_tag_columns && self.emmy_doc.align_reference_tags
    }

    pub fn trailing_table_comma(&self) -> TrailingComma {
        match self.output.trailing_table_separator {
            TrailingTableSeparator::Inherit => self.output.trailing_comma.clone(),
            TrailingTableSeparator::Never => TrailingComma::Never,
            TrailingTableSeparator::Multiline => TrailingComma::Multiline,
            TrailingTableSeparator::Always => TrailingComma::Always,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IndentConfig {
    pub kind: IndentKind,
    pub width: usize,
}

impl Default for IndentConfig {
    fn default() -> Self {
        Self {
            kind: IndentKind::Space,
            width: 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutConfig {
    pub max_line_width: usize,
    pub max_blank_lines: usize,
    pub table_expand: ExpandStrategy,
    pub call_args_expand: ExpandStrategy,
    pub func_params_expand: ExpandStrategy,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            max_line_width: 120,
            max_blank_lines: 1,
            table_expand: ExpandStrategy::Auto,
            call_args_expand: ExpandStrategy::Auto,
            func_params_expand: ExpandStrategy::Auto,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    pub insert_final_newline: bool,
    pub trailing_comma: TrailingComma,
    pub trailing_table_separator: TrailingTableSeparator,
    pub quote_style: QuoteStyle,
    pub single_arg_call_parens: SingleArgCallParens,
    pub simple_lambda_single_line: SimpleLambdaSingleLine,
    pub end_of_line: EndOfLine,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            insert_final_newline: true,
            trailing_comma: TrailingComma::Never,
            trailing_table_separator: TrailingTableSeparator::Inherit,
            quote_style: QuoteStyle::Preserve,
            single_arg_call_parens: SingleArgCallParens::Preserve,
            simple_lambda_single_line: SimpleLambdaSingleLine::Preserve,
            end_of_line: EndOfLine::LF,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SpacingConfig {
    pub space_before_call_paren: bool,
    pub space_before_func_paren: bool,
    pub space_inside_braces: bool,
    pub space_inside_parens: bool,
    pub space_inside_brackets: bool,
    pub space_around_math_operator: bool,
    pub space_around_concat_operator: bool,
    pub space_around_assign_operator: bool,
}

impl Default for SpacingConfig {
    fn default() -> Self {
        Self {
            space_before_call_paren: false,
            space_before_func_paren: false,
            space_inside_braces: true,
            space_inside_parens: false,
            space_inside_brackets: false,
            space_around_math_operator: true,
            space_around_concat_operator: true,
            space_around_assign_operator: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CommentConfig {
    pub align_line_comments: bool,
    pub align_in_statements: bool,
    pub align_in_table_fields: bool,
    pub align_in_call_args: bool,
    pub align_in_params: bool,
    pub align_across_standalone_comments: bool,
    pub align_same_kind_only: bool,
    pub space_after_comment_dash: bool,
    pub line_comment_min_spaces_before: usize,
    pub line_comment_min_column: usize,
}

impl Default for CommentConfig {
    fn default() -> Self {
        Self {
            align_line_comments: true,
            align_in_statements: false,
            align_in_table_fields: true,
            align_in_call_args: true,
            align_in_params: true,
            align_across_standalone_comments: false,
            align_same_kind_only: false,
            space_after_comment_dash: true,
            line_comment_min_spaces_before: 1,
            line_comment_min_column: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmmyDocConfig {
    pub align_tag_columns: bool,
    pub align_declaration_tags: bool,
    pub align_reference_tags: bool,
    pub tag_spacing: usize,
    pub space_after_description_dash: bool,
}

impl Default for EmmyDocConfig {
    fn default() -> Self {
        Self {
            align_tag_columns: true,
            align_declaration_tags: true,
            align_reference_tags: true,
            tag_spacing: 1,
            space_after_description_dash: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AlignConfig {
    pub continuous_assign_statement: bool,
    pub table_field: bool,
}

impl Default for AlignConfig {
    fn default() -> Self {
        Self {
            continuous_assign_statement: false,
            table_field: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum IndentKind {
    Tab,
    Space,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrailingComma {
    Never,
    Multiline,
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrailingTableSeparator {
    Inherit,
    Never,
    Multiline,
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum QuoteStyle {
    Preserve,
    Double,
    Single,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SingleArgCallParens {
    Preserve,
    Always,
    Omit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SimpleLambdaSingleLine {
    Preserve,
    Always,
    Never,
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

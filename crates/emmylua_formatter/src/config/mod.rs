use serde::{Deserialize, Serialize};

/// Formatter root config.
///
/// Each field controls one area of formatting behavior and maps directly to a
/// subsection in the external config file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LuaFormatConfig {
    /// Lua parser syntax-level settings.
    pub syntax: SyntaxConfig,
    /// Indentation style and width.
    pub indent: IndentConfig,
    /// High-level layout decisions such as line width and expand strategies.
    pub layout: LayoutConfig,
    /// Output shape such as trailing separators, quotes, and final newline.
    pub output: OutputConfig,
    /// Token-level spacing rules for calls, operators, and delimiters.
    pub spacing: SpacingConfig,
    /// Normal comment formatting and line-comment alignment behavior.
    pub comments: CommentConfig,
    /// EmmyLua doc-comment specific formatting behavior.
    pub emmy_doc: EmmyDocConfig,
    /// Cross-line alignment features for assignments and table fields.
    pub align: AlignConfig,
}

/// Lua parser syntax settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SyntaxConfig {
    /// Lua grammar level used for parsing before formatting.
    ///
    /// If omitted in config files, the formatter defaults to `Lua55`.
    pub level: LuaSyntaxLevel,
}

impl Default for SyntaxConfig {
    fn default() -> Self {
        Self {
            level: LuaSyntaxLevel::Lua55,
        }
    }
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

    pub fn should_align_emmy_doc_multiline_alias_descriptions(&self) -> bool {
        self.emmy_doc.align_tag_columns && self.emmy_doc.align_multiline_alias_descriptions
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

/// Indentation settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IndentConfig {
    /// Whether indentation uses tabs or spaces.
    pub kind: IndentKind,
    /// Width of one indentation level.
    ///
    /// When `kind = Space`, this is the number of spaces per indent level.
    /// When `kind = Tab`, this value is kept for config completeness but the
    /// emitted indent string is still a single tab.
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

/// Layout strategy settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutConfig {
    /// Preferred maximum rendered line width.
    ///
    /// The formatter will try to keep grouped output within this width and will
    /// break expressions, tables, and argument lists when needed.
    pub max_line_width: usize,
    /// Maximum number of consecutive blank lines preserved between statements.
    pub max_blank_lines: usize,
    /// Expansion strategy for table constructors.
    ///
    /// `Auto` keeps short tables inline when possible and expands when shape or
    /// width requires it.
    pub table_expand: ExpandStrategy,
    /// Expansion strategy for call argument lists.
    pub call_args_expand: ExpandStrategy,
    /// Expansion strategy for function parameter lists.
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

/// Output style settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    /// Whether to ensure the formatted file ends with a newline.
    pub insert_final_newline: bool,
    /// General trailing comma strategy used by supported multiline constructs.
    pub trailing_comma: TrailingComma,
    /// Trailing separator strategy specifically for table constructors.
    ///
    /// `Inherit` reuses `trailing_comma`.
    pub trailing_table_separator: TrailingTableSeparator,
    /// Preferred quote style for short strings.
    pub quote_style: QuoteStyle,
    /// Whether one-argument calls may omit parentheses for string/table args.
    pub single_arg_call_parens: SingleArgCallParens,
    /// Policy for collapsing simple `function(...) return expr end` lambdas.
    pub simple_lambda_single_line: SimpleLambdaSingleLine,
    /// Target line ending sequence used in formatted output.
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

/// Token spacing settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SpacingConfig {
    /// Insert a space before a normal call's `(`.
    ///
    /// Example: `foo ()` instead of `foo()`.
    pub space_before_call_paren: bool,
    /// Insert a space before a function declaration or anonymous function's `(`.
    ///
    /// Example: `function foo ()` instead of `function foo()`.
    pub space_before_func_paren: bool,
    /// Insert spaces just inside `{` and `}` for inline tables.
    ///
    /// Example: `{ a = 1 }` instead of `{a = 1}`.
    pub space_inside_braces: bool,
    /// Insert spaces just inside `(` and `)` where applicable.
    pub space_inside_parens: bool,
    /// Insert spaces just inside `[` and `]` where applicable.
    pub space_inside_brackets: bool,
    /// Insert spaces around arithmetic and comparison-style math operators.
    pub space_around_math_operator: bool,
    /// Insert spaces around the concatenation operator.
    pub space_around_concat_operator: bool,
    /// Insert spaces around assignment operators.
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

/// Normal comment formatting settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CommentConfig {
    /// Master switch for `-- trailing` line-comment alignment.
    pub align_line_comments: bool,
    /// Allow line-comment alignment across normal statements.
    pub align_in_statements: bool,
    /// Allow line-comment alignment inside table fields.
    pub align_in_table_fields: bool,
    /// Allow line-comment alignment inside call arguments.
    pub align_in_call_args: bool,
    /// Allow line-comment alignment inside parameter lists.
    pub align_in_params: bool,
    /// Whether standalone comment lines may participate in an alignment group.
    pub align_across_standalone_comments: bool,
    /// Restrict alignment groups to entries of the same statement kind.
    pub align_same_kind_only: bool,
    /// Whether ordinary `-- comment` lines insert a space after `--`.
    ///
    /// This only affects normal comments, not EmmyLua doc comments.
    pub space_after_comment_dash: bool,
    /// Minimum spaces inserted before an inline normal line comment.
    pub line_comment_min_spaces_before: usize,
    /// Preferred minimum column for aligned inline normal line comments.
    ///
    /// `0` disables column targeting and only uses
    /// `line_comment_min_spaces_before`.
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

/// EmmyLua doc-comment formatting settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmmyDocConfig {
    /// Master switch for EmmyLua tag-column alignment.
    ///
    /// When disabled, declaration/reference tag alignment helpers are ignored.
    pub align_tag_columns: bool,
    /// Align declaration-style tags such as `@class`, `@alias`, `@type`,
    /// `@generic`, and `@overload`.
    pub align_declaration_tags: bool,
    /// Align reference-style tags such as `@param`, `@field`, and `@return`.
    pub align_reference_tags: bool,
    /// Align the description column of multiline `@alias` continuations.
    pub align_multiline_alias_descriptions: bool,
    /// Whether EmmyLua tag lines keep a space between `---` and `@`.
    ///
    /// Example: `--- @enum MyEnum` when enabled, `---@enum MyEnum` when
    /// disabled.
    ///
    /// This only affects tag lines. It does not affect plain description lines
    /// such as `--- text`.
    pub space_between_tag_columns: bool,
    /// Whether plain EmmyLua description lines insert a space after `---`.
    ///
    /// This affects non-tag doc lines like `--- description` versus
    /// `---description`.
    ///
    /// It does not affect tag lines such as `---@class` / `--- @class`; those
    /// are controlled by `space_between_tag_columns`.
    pub space_after_description_dash: bool,
}

impl Default for EmmyDocConfig {
    fn default() -> Self {
        Self {
            align_tag_columns: true,
            align_declaration_tags: true,
            align_reference_tags: true,
            align_multiline_alias_descriptions: true,
            space_between_tag_columns: false,
            space_after_description_dash: true,
        }
    }
}

/// Extra alignment features beyond ordinary wrapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AlignConfig {
    /// Align consecutive assignment statements by their `=` when possible.
    pub continuous_assign_statement: bool,
    /// Align table field assignments by their `=` when the formatter detects an
    /// alignment group.
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum LuaSyntaxLevel {
    Lua51,
    Lua52,
    Lua53,
    Lua54,
    #[default]
    Lua55,
    LuaJIT,
}

impl From<LuaSyntaxLevel> for emmylua_parser::LuaLanguageLevel {
    fn from(level: LuaSyntaxLevel) -> Self {
        match level {
            LuaSyntaxLevel::Lua51 => emmylua_parser::LuaLanguageLevel::Lua51,
            LuaSyntaxLevel::Lua52 => emmylua_parser::LuaLanguageLevel::Lua52,
            LuaSyntaxLevel::Lua53 => emmylua_parser::LuaLanguageLevel::Lua53,
            LuaSyntaxLevel::Lua54 => emmylua_parser::LuaLanguageLevel::Lua54,
            LuaSyntaxLevel::Lua55 => emmylua_parser::LuaLanguageLevel::Lua55,
            LuaSyntaxLevel::LuaJIT => emmylua_parser::LuaLanguageLevel::LuaJIT,
        }
    }
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

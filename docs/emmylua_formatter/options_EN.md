# EmmyLua Formatter Options

[中文文档](./options_CN.md)

This document describes the public formatter configuration groups and the intended effect of each option.

## Configuration File Discovery

`luafmt` and the library path-aware helpers support nearest-config discovery for:

- `.luafmt.toml`
- `luafmt.toml`

Supported explicit config formats are:

- TOML
- JSON
- YAML

## indent

- `kind`: `Space` or `Tab`
- `width`: logical indent width

Default:

```toml
[indent]
kind = "Space"
width = 4
```

## layout

- `max_line_width`: preferred print width
- `max_blank_lines`: maximum consecutive blank lines retained
- `table_expand`: `Never`, `Always`, or `Auto`
- `call_args_expand`: `Never`, `Always`, or `Auto`
- `func_params_expand`: `Never`, `Always`, or `Auto`

Default:

```toml
[layout]
max_line_width = 120
max_blank_lines = 1
table_expand = "Auto"
call_args_expand = "Auto"
func_params_expand = "Auto"
```

Behavior notes:

- `Auto` lets the formatter compare flat and broken candidates.
- Sequence-like structures can now choose between fill, packed, aligned, and one-per-line layouts when applicable.
- Binary-expression chains and statement expression lists may prefer a balanced packed layout when it keeps the same line count but avoids ragged trailing lines.

## output

- `insert_final_newline`
- `trailing_comma`: `Never`, `Multiline`, or `Always`
- `trailing_table_separator`: `Inherit`, `Never`, `Multiline`, or `Always`
- `quote_style`: `Preserve`, `Double`, or `Single`
- `single_arg_call_parens`: `Preserve`, `Always`, or `Omit`
- `simple_lambda_single_line`: `Preserve`, `Always`, or `Never`
- `end_of_line`: `LF` or `CRLF`

Default:

```toml
[output]
insert_final_newline = true
trailing_comma = "Never"
trailing_table_separator = "Inherit"
quote_style = "Preserve"
single_arg_call_parens = "Preserve"
simple_lambda_single_line = "Preserve"
end_of_line = "LF"
```

Behavior notes:

- `trailing_comma` is the general trailing-comma policy for sequence-like constructs.
- `trailing_table_separator` overrides that policy for tables only. `Inherit` keeps using `trailing_comma`.
- `quote_style` only rewrites normal short strings when it is safe to do so. Long strings and other string forms are preserved.
- Quote rewriting works from the raw token text, checks for unescaped occurrences of the target delimiter, and only adjusts the minimal delimiter escaping needed to preserve semantics.
- `single_arg_call_parens = "Omit"` only removes parentheses for Lua-valid single-string and single-table calls.
- `simple_lambda_single_line = "Preserve"` only keeps an eligible simple lambda on one line when the input was already inline.
- `simple_lambda_single_line = "Always"` collapses an eligible simple lambda back to `function(...) return expr end` when it fits within the configured width.
- `simple_lambda_single_line = "Never"` disables the simple inline lambda fast path and always formats the closure body on multiple lines.

## spacing

- `space_before_call_paren`
- `space_before_func_paren`
- `space_inside_braces`
- `space_inside_parens`
- `space_inside_brackets`
- `space_around_math_operator`
- `space_around_concat_operator`
- `space_around_assign_operator`

These options control token spacing only. They do not override larger layout decisions such as whether an expression list should break.

## comments

- `align_line_comments`
- `align_in_statements`
- `align_in_table_fields`
- `align_in_call_args`
- `align_in_params`
- `align_across_standalone_comments`
- `align_same_kind_only`
- `space_after_comment_dash`
- `line_comment_min_spaces_before`
- `line_comment_min_column`

Default:

```toml
[comments]
align_line_comments = true
align_in_statements = false
align_in_table_fields = true
align_in_call_args = true
align_in_params = true
align_across_standalone_comments = false
align_same_kind_only = false
space_after_comment_dash = true
line_comment_min_spaces_before = 1
line_comment_min_column = 0
```

Behavior notes:

- Statement comment alignment is disabled by default.
- Table, call-arg, and parameter trailing-comment alignment are input-driven. Extra spacing in the original source is treated as alignment intent.
- Standalone comments usually break alignment groups.
- Table-field trailing-comment alignment is scoped to contiguous subgroups rather than the whole table.
- `space_after_comment_dash` only inserts one space for plain comments such as `--comment` when there is no gap after the prefix already; comments with larger existing gaps are preserved.

## emmy_doc

- `align_tag_columns`
- `align_declaration_tags`
- `align_reference_tags`
- `tag_spacing`
- `space_after_description_dash`

Default:

```toml
[emmy_doc]
align_tag_columns = true
align_declaration_tags = true
align_reference_tags = true
tag_spacing = 1
space_after_description_dash = true
```

Structured handling currently covers `@param`, `@field`, `@return`, `@class`, `@alias`, `@type`, `@generic`, and `@overload`.

## align

- `continuous_assign_statement`
- `table_field`

Default:

```toml
[align]
continuous_assign_statement = false
table_field = true
```

Behavior notes:

- Continuous assignment alignment is disabled by default.
- Table-field alignment is enabled, but only activates when the source already shows extra post-`=` spacing that indicates alignment intent.

## Recommended Starting Point

```toml
[layout]
max_line_width = 100
table_expand = "Auto"
call_args_expand = "Auto"
func_params_expand = "Auto"

[comments]
align_in_statements = false
align_in_table_fields = true
align_in_call_args = true
align_in_params = true

[align]
continuous_assign_statement = false
table_field = true
```

## Complete Default Config

```toml
[indent]
kind = "Space"
width = 4

[layout]
max_line_width = 120
max_blank_lines = 1
table_expand = "Auto"
call_args_expand = "Auto"
func_params_expand = "Auto"

[output]
insert_final_newline = true
trailing_comma = "Never"
trailing_table_separator = "Inherit"
quote_style = "Preserve"
single_arg_call_parens = "Preserve"
simple_lambda_single_line = "Preserve"
end_of_line = "LF"

[spacing]
space_before_call_paren = false
space_before_func_paren = false
space_inside_braces = true
space_inside_parens = false
space_inside_brackets = false
space_around_math_operator = true
space_around_concat_operator = true
space_around_assign_operator = true

[comments]
align_line_comments = true
align_in_statements = false
align_in_table_fields = true
align_in_call_args = true
align_in_params = true
align_across_standalone_comments = false
align_same_kind_only = false
space_after_comment_dash = true
line_comment_min_spaces_before = 1
line_comment_min_column = 0

[emmy_doc]
align_tag_columns = true
align_declaration_tags = true
align_reference_tags = true
tag_spacing = 1
space_after_description_dash = true

[align]
continuous_assign_statement = false
table_field = true
```

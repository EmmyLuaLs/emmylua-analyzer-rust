# EmmyLua Formatter Options

[中文文档](./options_CN.md)

This document describes the public formatter configuration groups and the intended effect of each option.

## Configuration File Discovery

`luafmt` and the library path-aware helpers support nearest-config discovery for:

- `.luafmt.toml`
- `luafmt.toml`

When input comes from stdin and no source path is available, `luafmt` starts discovery from the current working directory.

Supported explicit config formats are:

- TOML
- JSON
- YAML

## syntax

- `level`: Lua grammar level. Supported values are `Lua51`, `Lua52`, `Lua53`, `Lua54`, `Lua55`, and `LuaJIT`

Default:

```toml
[syntax]
level = "Lua55"
```

Notes:

- This option controls which Lua grammar is used during parsing before formatting.
- If `syntax.level` is omitted from the config file, the formatter defaults to `Lua55`.
- The CLI `--level` flag overrides `syntax.level` from config.

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
- `prefer_call_args_layout_from_source`: prefer keeping the source layout goal for explicit multiline call argument lists
- `prefer_table_layout_from_source`: prefer keeping the source layout goal for explicit multiline pure-array tables
- `prefer_chain_break_on_statement_tail`: prefer multiline breaking for fluent chains in statement-tail position

Default:

```toml
[layout]
max_line_width = 120
max_blank_lines = 1
table_expand = "Auto"
call_args_expand = "Auto"
func_params_expand = "Auto"
prefer_call_args_layout_from_source = false
prefer_table_layout_from_source = false
prefer_chain_break_on_statement_tail = false
```

Behavior notes:

- `Auto` lets the formatter compare flat and broken candidates.
- `prefer_call_args_layout_from_source = true` only affects call argument lists under `call_args_expand = "Auto"`.
- When a call argument list is already multiline in the source, the formatter keeps it expanded instead of reflowing it back into a compact layout.
- `prefer_table_layout_from_source = true` only affects table constructors under `table_expand = "Auto"`.
- It applies only to pure-array tables, meaning tables whose fields are all positional values without keys.
- When such a table is already multiline in the source, the formatter keeps it expanded instead of reflowing it back into a compact layout.
- Sequence-like structures can now choose between fill, packed, aligned, and one-per-line layouts when applicable.
- Binary-expression chains and statement expression lists may prefer a balanced packed layout when it keeps the same line count but avoids ragged trailing lines.
- `prefer_chain_break_on_statement_tail = true` only applies to the last direct expression in a statement, including standalone call statements; when that expression is a long enough fluent chain, the formatter prefers chain-style breaking.
- The chain head is defined as the `root`, plus any leading namespace or field accesses, plus the first call segment. For example, the head of `Builder:new():add():add()` is `Builder:new()`, while the head of `vim.api.nvim_set_keymap(...)` is the whole `vim.api.nvim_set_keymap(...)` call.
- The break start is defined as the first continuation segment after that head. In other words, only segments that continue after the first call are eligible for chain-style line breaking; a namespace-qualified terminal call is not treated as a fluent chain by this option.

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
- `space_before_lambda_func_paren`
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
- `align_multiline_alias_descriptions`
- `space_between_tag_columns`
- `space_after_description_dash`
- `compact_type_or`

Default:

```toml
[emmy_doc]
align_tag_columns = true
align_declaration_tags = true
align_reference_tags = true
align_multiline_alias_descriptions = true
space_between_tag_columns = false
space_after_description_dash = true
compact_type_or = false
```

Structured handling currently covers `@param`, `@field`, `@return`, `@class`, `@alias`, `@type`, `@generic`, and `@overload`.

- `align_multiline_alias_descriptions` is enabled by default and aligns the `# description` column in multiline `@alias` blocks such as `--- | value # description`.
- `space_between_tag_columns` controls whether EmmyLua tag lines keep a space between `---` and `@`, for example `--- @enum MyEnum` versus `---@enum MyEnum`. The current default is `false`, so tag lines format as `---@tag` unless configured otherwise.
- `space_after_description_dash` only affects plain doc description lines such as `--- text` versus `---text`, not tag-line prefixes.
- `compact_type_or` controls whether EmmyLua union types keep spaces around `|`, for example `string | integer` versus `string|integer`. The default is `false`.

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
space_before_lambda_func_paren = true
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
align_multiline_alias_descriptions = true
tag_spacing = 1
space_after_description_dash = true

[align]
continuous_assign_statement = false
table_field = true
```

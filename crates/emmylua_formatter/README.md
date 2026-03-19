# EmmyLua Formatter

EmmyLua Formatter is an experimental Lua/EmmyLua formatter built on a DocIR-style pipeline:

- parse source into syntax nodes
- convert syntax into formatting IR
- print IR back to text with width-aware layout decisions

The crate already supports practical formatting for statements, expressions, tables, comments, and a growing subset of EmmyLua doc tags.

Trivia-aware formatter redesign notes are documented in `TRIVIA_FORMATTING_DESIGN.md`.

## Current Focus

Recent work has concentrated on formatter stability and configurability, especially around alignment-sensitive output:

- trailing line comment alignment with per-scope switches
- assignment spacing control
- shebang preservation
- EmmyLua doc-tag normalization and alignment
- conservative fallback for complex doc-tag syntax

## Comment Alignment

Trailing line comments are configured under `LuaFormatConfig.comments`:

- `align_line_comments`
- `align_in_statements`
- `align_in_table_fields`
- `align_in_params`
- `align_across_standalone_comments`
- `align_same_kind_only`
- `line_comment_min_spaces_before`
- `line_comment_min_column`

## EmmyLua Doc Tags

The formatter currently has structured handling for:

- `@param`
- `@field`
- `@return`
- `@class`
- `@alias`
- `@type`
- `@generic`
- `@overload`

Alignment behavior is controlled under `LuaFormatConfig.emmy_doc`:

- `align_tag_columns`
- `align_declaration_tags`
- `align_reference_tags`
- `tag_spacing`
- `space_after_description_dash`

Notes:

- declaration tags are `@class`, `@alias`, `@type`, `@generic`, `@overload`
- reference tags are `@param`, `@field`, `@return`
- `@alias` keeps its original single-line body text and only participates in description-column alignment
- `space_after_description_dash` controls whether plain doc lines render as `--- text` or `---text`
- multiline or complex doc-tag forms fall back to raw preservation instead of risky rewriting

## luafmt

The CLI now supports:

- `--config <FILE>` with `toml`, `json`, `yml`, or `yaml`
- automatic discovery of `.luafmt.toml` or `luafmt.toml`
- `--dump-default-config` to print a starter TOML config
- recursive directory input
- `--include` / `--exclude` glob filters
- `.luafmtignore` support for batch formatting

Typical usage:

```powershell
luafmt src --write
luafmt . --check --exclude "vendor/**"
luafmt game --list-different
```

## Library API

The crate now exposes workspace-friendly helpers so the language server or other callers do not need to shell out to `luafmt`:

- `resolve_config_for_path` to load the nearest formatter config for a file
- `format_text_for_path` to format in-memory text with path-based config discovery
- `format_file` to format a file directly
- `collect_lua_files` to gather `lua` and `luau` files from directories with ignore support

Example:

```rust
use std::path::Path;

use emmylua_formatter::{format_text_for_path, resolve_config_for_path};

let source_path = Path::new("workspace/scripts/main.lua");
let resolved = resolve_config_for_path(Some(source_path), None)?;
let result = format_text_for_path("local x=1\n", Some(source_path), None)?;

assert_eq!(resolved.source_path.is_some(), true);
assert!(result.output.changed);
```

## Example Config

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
align_in_statements = true
align_in_table_fields = true
align_in_params = true
align_across_standalone_comments = true
align_same_kind_only = false
line_comment_min_spaces_before = 1
line_comment_min_column = 0

[emmy_doc]
align_tag_columns = true
align_declaration_tags = true
align_reference_tags = true
tag_spacing = 1
space_after_description_dash = true

[align]
continuous_assign_statement = true
table_field = true
```

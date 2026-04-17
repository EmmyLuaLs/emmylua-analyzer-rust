# EmmyLua Formatter

EmmyLua Formatter is the Lua and EmmyLua formatter used by the EmmyLua Analyzer Rust workspace.

It focuses on three practical goals:

- deterministic output across repeated runs
- width-aware layout selection instead of one fixed broken shape
- structured handling for common EmmyLua doc tags without normalizing unrelated comment text too aggressively

The crate provides both:

- a `luafmt` CLI for files, directories, and stdin
- a library API for callers that want to resolve config, collect files, and format in-process

## Current Capabilities

The formatter currently handles:

- normal Lua statements and expressions
- tables, chained calls, function parameter lists, and statement expression lists
- trailing line comments and input-sensitive comment alignment
- common EmmyLua tags such as `@param`, `@field`, `@return`, `@class`, `@alias`, `@type`, `@generic`, and `@overload`

The layout engine does not rely on a single multiline fallback. For sequence-like constructs it can compare several candidate layouts and choose between:

- flat output when everything fits
- progressive fill layouts
- more balanced packed layouts
- full one-item-per-line expansion when narrower candidates are clearly worse

That behavior is used in places like call arguments, function parameters, table fields, binary-expression chains, assignment right-hand sides, `return` lists, and loop headers.

## CLI

The workspace binary is `luafmt`.

Typical usage:

```powershell
luafmt src --write
luafmt . --check --exclude "vendor/**"
Get-Content script.lua | luafmt --stdin
Get-Content script.lua | luafmt --stdin --output formatted.lua
```

Main CLI features:

- reads files, directories, or stdin
- `--write`, `--check`, and `--list-different`
- `--config <FILE>` for explicit TOML, JSON, YML, or YAML config files
- automatic discovery of `.luafmt.toml` and `luafmt.toml`
- `.luafmtignore` support when walking directories
- `--include` and `--exclude` glob filters
- `--dump-default-config`
- `--color auto|always|never`
- `--diff-style marker|git`
- `--level <Lua51|Lua52|Lua53|Lua54|Lua55|LuaJIT>` as an explicit parser-level override

Config discovery rules:

- when formatting a file path, `luafmt` searches upward from that path
- when formatting stdin without a source path, `luafmt` searches upward from the current working directory
- when `--config` is provided, that file wins

## Configuration

The public config root is `LuaFormatConfig`.

```rust
pub struct LuaFormatConfig {
	pub syntax: SyntaxConfig,
	pub indent: IndentConfig,
	pub layout: LayoutConfig,
	pub output: OutputConfig,
	pub spacing: SpacingConfig,
	pub comments: CommentConfig,
	pub emmy_doc: EmmyDocConfig,
	pub align: AlignConfig,
}
```

Important defaults:

```toml
[syntax]
level = "Lua55"

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
space_between_tag_columns = false
space_after_description_dash = true

[align]
continuous_assign_statement = false
table_field = true
```

Notes that matter in practice:

- `syntax.level` controls parser grammar and defaults to `Lua55`
- CLI `--level` overrides `syntax.level` from config
- `emmy_doc.space_between_tag_columns = false` means the default tag prefix is `---@tag`, not `--- @tag`
- `emmy_doc.space_after_description_dash` only affects plain doc description lines such as `--- text` vs `---text`
- trailing line comment alignment is intentionally conservative and often only activates when the input already signals alignment intent

To inspect the exact current default config:

```powershell
luafmt --dump-default-config
```

## Library API

The crate exposes two useful layers.

Low-level formatting helpers:

- `check_text`
- `format_text`
- `reformat_lua_code`
- `reformat_chunk`

Path-aware workspace helpers:

- `resolve_config_for_path`
- `discover_config_path`
- `load_format_config`
- `parse_format_config`
- `check_text_for_path`
- `format_text_for_path`
- `check_file`
- `format_file`
- `collect_lua_files`

If you want library behavior that matches `luafmt`, resolve config first and then use the resolved syntax level:

```rust
use std::path::Path;

use emmylua_formatter::{check_text, resolve_config_for_path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let source_path = Path::new("workspace/scripts/main.lua");
	let source = "---@param value string\nlocal function f(value) end\n";

	let resolved = resolve_config_for_path(Some(source_path), None)?;
	let result = check_text(source, resolved.config.syntax.level.into(), &resolved.config);

	assert!(resolved.source_path.is_some());
	assert!(result.changed);
	Ok(())
}
```

That pattern is important because the path-aware config can now carry the Lua syntax level through `syntax.level`.

## Example Config

```toml
[syntax]
level = "Lua55"

[indent]
kind = "Space"
width = 4

[layout]
max_line_width = 100
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
align_multiline_alias_descriptions = true
space_between_tag_columns = false
space_after_description_dash = true

[align]
continuous_assign_statement = false
table_field = true
```

## More Documentation

Additional formatter docs live in the workspace documentation directory:

- `../../docs/emmylua_formatter/README_EN.md`
- `../../docs/emmylua_formatter/README_CN.md`
- `../../docs/emmylua_formatter/options_EN.md`
- `../../docs/emmylua_formatter/options_CN.md`
- `../../docs/emmylua_formatter/examples_EN.md`
- `../../docs/emmylua_formatter/examples_CN.md`
- `../../docs/emmylua_formatter/profiles_EN.md`
- `../../docs/emmylua_formatter/profiles_CN.md`
- `../../docs/emmylua_formatter/tutorial_EN.md`
- `../../docs/emmylua_formatter/tutorial_CN.md`

For real before-and-after output, start with the examples and options documents.

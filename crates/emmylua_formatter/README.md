# EmmyLua Formatter

EmmyLua Formatter is the structured Lua and EmmyLua formatter in the EmmyLua Analyzer Rust workspace. It is designed for deterministic output, conservative comment handling, and width-aware layout decisions that remain stable under repeated formatting.

The formatter pipeline is built in three stages:

1. Parse source text into syntax nodes.
2. Lower syntax into DocIR.
3. Print DocIR back to text with configurable layout selection.

The current implementation covers statements, expressions, table literals, chained calls, binary-expression chains, trailing comments, and a practical subset of EmmyLua doc tags.

Sequence-layout redesign notes are documented in `SEQUENCE_LAYOUT_DESIGN.md`.

## Design Goals

The formatter currently prioritizes the following properties:

- stable formatting for repeated runs
- conservative preservation around comments and ambiguous syntax
- width-aware packing before fully expanded one-item-per-line output
- configuration that is narrow in scope and predictable in effect

Recent layout work introduced candidate-based selection for sequence-like constructs. Instead of committing to a single hard-coded broken layout, the formatter can compare fill, packed, aligned, and one-per-line candidates and choose the best result for the active width.

## Layout Behavior

The formatter now uses candidate selection in several important paths:

- call arguments
- function parameters
- table fields
- binary-expression chains
- statement expression lists used by `return`, assignment right-hand sides, and loop headers

In practice this means the formatter can prefer:

- a flat layout when everything fits
- progressive fill when a compact multi-line layout is sufficient
- a more balanced packed layout when it avoids ragged trailing lines
- one-item-per-line expansion only when the narrower layouts are clearly worse

Comment-sensitive paths remain conservative. Standalone comments still block aggressive repacking, and trailing line comment alignment only activates when the input already shows alignment intent.

## Configuration Overview

The public formatter configuration is exposed through `LuaFormatConfig`:

- `indent`
- `layout`
- `output`
- `spacing`
- `comments`
- `emmy_doc`
- `align`

Key defaults:

- `layout.max_line_width = 120`
- `layout.table_expand = "Auto"`
- `layout.call_args_expand = "Auto"`
- `layout.func_params_expand = "Auto"`
- `output.trailing_comma = "Never"`
- `comments.align_in_statements = false`
- `align.continuous_assign_statement = false`
- `align.table_field = true`

These defaults intentionally favor conservative rewrites. Alignment-heavy output is not enabled broadly unless the source already indicates that alignment should be preserved.

## Comment Alignment

Trailing line comment behavior is configured under `LuaFormatConfig.comments`:

- `align_line_comments`
- `align_in_statements`
- `align_in_table_fields`
- `align_in_call_args`
- `align_in_params`
- `align_across_standalone_comments`
- `align_same_kind_only`
- `line_comment_min_spaces_before`
- `line_comment_min_column`

Current alignment rules are intentionally scoped:

- statement alignment is disabled by default
- call-arg, parameter, and table-field alignment only activate when the input already contains extra spacing that signals alignment intent
- standalone comments break alignment groups by default
- table comment alignment is limited to contiguous subgroups rather than the entire table body

## EmmyLua Doc Tags

Structured handling currently exists for:

- `@param`
- `@field`
- `@return`
- `@class`
- `@alias`
- `@type`
- `@generic`
- `@overload`

Doc-tag behavior is controlled under `LuaFormatConfig.emmy_doc`:

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
- multiline or complex doc-tag forms fall back to raw preservation instead of speculative rewriting

## CLI

The `luafmt` binary supports:

- `--config <FILE>` with `toml`, `json`, `yml`, or `yaml`
- automatic discovery of `.luafmt.toml` or `luafmt.toml`
- `--dump-default-config`
- recursive directory input
- `--include` and `--exclude` glob filters
- `.luafmtignore`
- `--check` and `--list-different`
- `--color auto|always|never`
- `--diff-style marker|git`

Typical usage:

```powershell
luafmt src --write
luafmt . --check --exclude "vendor/**"
luafmt game --list-different
```

## Library API

The crate exposes workspace-friendly helpers so callers do not need to shell out to `luafmt`:

- `resolve_config_for_path`
- `format_text_for_path`
- `check_text_for_path`
- `format_file`
- `check_file`
- `collect_lua_files`

Example:

```rust
use std::path::Path;

use emmylua_formatter::{format_text_for_path, resolve_config_for_path};

let source_path = Path::new("workspace/scripts/main.lua");
let resolved = resolve_config_for_path(Some(source_path), None)?;
let result = format_text_for_path("local x=1\n", Some(source_path), None)?;

assert!(resolved.source_path.is_some());
assert!(result.output.changed);
```

## Documentation

Additional formatter documentation is available in the workspace docs directory:

- `../../docs/emmylua_formatter/README_EN.md`
- `../../docs/emmylua_formatter/examples_EN.md`
- `../../docs/emmylua_formatter/options_EN.md`
- `../../docs/emmylua_formatter/profiles_EN.md`
- `../../docs/emmylua_formatter/tutorial_EN.md`

The examples page is the best place to review actual before-and-after output for tables, call arguments, binary chains, and statement expression lists.


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
align_in_statements = false
align_in_table_fields = true
align_in_call_args = true
align_in_params = true
align_across_standalone_comments = false
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
continuous_assign_statement = false
table_field = true
```

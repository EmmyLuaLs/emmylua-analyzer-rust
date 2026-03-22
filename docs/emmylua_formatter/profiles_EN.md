# EmmyLua Formatter Recommended Profiles

[中文文档](./profiles_CN.md)

This page provides recommended formatter configurations for common team styles. These profiles are not special built-in modes. They are curated config examples based on the formatter's current behavior and defaults.

## 1. Conservative Default

Use this profile when the codebase has mixed style history, many comments, or frequent manual formatting.

Goals:

- minimize surprising rewrites
- keep alignment opt-in and input-driven
- prefer `Auto` for width-aware layout selection

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
align_across_standalone_comments = false

[align]
continuous_assign_statement = false
table_field = true
```

Recommended for:

- large existing repositories
- game scripts with hand-aligned comments
- teams that want stable formatting without strong alignment rules

## 2. Team Standard Profile

Use this profile when the team wants consistent formatting, but still prefers conservative comment handling.

Goals:

- unify width and spacing rules
- keep comments readable
- allow the formatter to choose flat, fill, packed, or one-per-line layouts automatically

```toml
[layout]
max_line_width = 88
table_expand = "Auto"
call_args_expand = "Auto"
func_params_expand = "Auto"

[spacing]
space_inside_braces = true
space_around_math_operator = true
space_around_concat_operator = true
space_around_assign_operator = true

[comments]
align_in_statements = false
align_in_table_fields = true
align_in_call_args = true
align_in_params = true

[align]
continuous_assign_statement = false
table_field = true
```

Recommended for:

- repositories using CI formatting checks
- teams that want predictable line breaking
- projects that want packed layouts but do not want aggressive alignment everywhere

## 3. Alignment-Sensitive Profile

Use this profile only when the codebase already relies heavily on visual alignment.

Goals:

- preserve intentionally aligned tables and comments
- retain explicit visual columns where they already exist

```toml
[layout]
max_line_width = 100
table_expand = "Auto"
call_args_expand = "Auto"
func_params_expand = "Auto"

[comments]
align_in_statements = true
align_in_table_fields = true
align_in_call_args = true
align_in_params = true
align_across_standalone_comments = false
align_same_kind_only = true
line_comment_min_spaces_before = 2

[align]
continuous_assign_statement = true
table_field = true
```

Recommended for:

- codebases with established visual columns
- generated or semi-generated script tables
- teams willing to review alignment-heavy diffs carefully

## Notes

- `Auto` is usually the best starting point for tables, call arguments, and parameter lists.
- The formatter now has balanced packed layouts for binary chains and statement expression lists. That means tighter line widths can still produce compact multi-line output without immediately collapsing into one item per line.
- If the repository contains many fragile comment blocks, start with the conservative profile and only enable more alignment after reviewing the diff quality.

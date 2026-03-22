# EmmyLua Formatter Tutorial

[中文文档](./tutorial_CN.md)

This tutorial covers the practical workflow for using the EmmyLua formatter from the command line, configuration files, and library APIs.

## 1. Install or Build

Build the formatter binary from this workspace:

```bash
cargo build --release -p emmylua_formatter
```

The formatter executable is `luafmt`.

## 2. Create a Config File

Create `.luafmt.toml` in the project root:

```toml
[layout]
max_line_width = 100
table_expand = "Auto"
call_args_expand = "Auto"
func_params_expand = "Auto"

[output]
quote_style = "Preserve"
trailing_table_separator = "Multiline"
single_arg_call_parens = "Preserve"

[comments]
align_in_statements = false
align_in_table_fields = true
align_in_call_args = true
align_in_params = true

[align]
continuous_assign_statement = false
table_field = true
```

The formatter discovers the nearest `.luafmt.toml` or `luafmt.toml` for each file.

If you want vertically expanded tables to carry trailing separators by default without changing call arguments or parameter lists, set `output.trailing_table_separator = "Multiline"`.

If you want to normalize short-string quoting, set `output.quote_style = "Double"` or `"Single"`. Long strings are preserved.

## 3. Format Files

Format a directory in place:

```bash
luafmt src --write
```

Check whether files would change:

```bash
luafmt . --check
```

List only changed paths:

```bash
luafmt . --list-different
```

Read from stdin:

```bash
cat script.lua | luafmt --stdin
```

## 4. Understand the Main Layout Modes

### Flat when possible

```lua
local point = { x = 1, y = 2 }
```

### Progressive fill for compact multi-line output

```lua
some_function(
    first_arg, second_arg, third_arg,
    fourth_arg
)
```

### Balanced packed layout for sequence-like structures

```lua
if alpha_beta_gamma + delta_theta
    + epsilon + zeta then
    work()
end
```

```lua
for key, value in first_long_expr,
    second_long_expr, third_long_expr,
    fourth_long_expr, fifth_long_expr do
    print(key, value)
end
```

### One item per line when narrower layouts are clearly worse

```lua
builder
    :set_name(name)
    :set_age(age)
    :build()
```

## 5. Comment Alignment

The formatter is conservative by default:

- statement comment alignment is off
- table, call-arg, and param comment alignment are input-driven
- standalone comments break alignment groups

This is intentional. It avoids manufacturing wide alignment blocks in files that were not written that way originally.

## 6. Use the Library API

```rust
use std::path::Path;

use emmylua_formatter::{check_text_for_path, format_text_for_path};

let path = Path::new("scripts/main.lua");
let formatted = format_text_for_path("local x=1\n", Some(path), None)?;
let checked = check_text_for_path("local x=1\n", Some(path), None)?;

assert!(formatted.output.changed);
assert!(checked.changed);
```

## 7. Recommended Team Workflow

1. Commit a shared `.luafmt.toml`.
2. Use `luafmt --check` in CI.
3. Keep alignment-related options conservative unless the codebase already relies on aligned comments or fields.
4. Prefer `Auto` expansion modes unless the project has a strong one-style policy.

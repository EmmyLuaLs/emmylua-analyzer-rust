# EmmyLua Formatter 教程

[English](./tutorial_EN.md)

本文档介绍 EmmyLua Formatter 的实际使用方式，包括命令行、配置文件以及库 API 集成。

## 1. 构建

在当前工作区中构建 formatter 可执行文件：

```bash
cargo build --release -p emmylua_formatter
```

生成的可执行文件名为 `luafmt`。

## 2. 编写配置文件

在项目根目录创建 `.luafmt.toml`：

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

格式化器会为每个文件向上查找最近的 `.luafmt.toml` 或 `luafmt.toml`。

## 3. 格式化文件

直接写回目录中的文件：

```bash
luafmt src --write
```

检查哪些文件会被改动：

```bash
luafmt . --check
```

只输出会变化的路径：

```bash
luafmt . --list-different
```

从标准输入读取：

```bash
cat script.lua | luafmt --stdin
```

## 4. 理解主要布局模式

### 能放一行时保持单行

```lua
local point = { x = 1, y = 2 }
```

### 需要换行时优先使用 progressive fill

```lua
some_function(
    first_arg, second_arg, third_arg,
    fourth_arg
)
```

### 在序列结构上选择更均衡的 packed 布局

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

### 只有更窄布局明显更差时才退到一项一行

```lua
builder
    :set_name(name)
    :set_age(age)
    :build()
```

## 5. 注释对齐

默认策略是保守的：

- statement 尾随注释对齐默认关闭
- table、调用参数、函数参数的尾随注释对齐是输入驱动的
- standalone comment 默认打断对齐分组

这样做是为了避免在原始代码没有体现对齐意图时，格式化器主动制造过宽的对齐块。

## 6. 库 API 集成

```rust
use std::path::Path;

use emmylua_formatter::{check_text_for_path, format_text_for_path};

let path = Path::new("scripts/main.lua");
let formatted = format_text_for_path("local x=1\n", Some(path), None)?;
let checked = check_text_for_path("local x=1\n", Some(path), None)?;

assert!(formatted.output.changed);
assert!(checked.changed);
```

## 7. 团队建议

1. 将统一的 `.luafmt.toml` 提交到仓库。
2. 在 CI 中使用 `luafmt --check`。
3. 对齐相关选项保持保守，除非代码库本身已经普遍依赖对齐风格。
4. 除非项目有非常强的统一风格要求，否则优先使用 `Auto` 扩展模式。

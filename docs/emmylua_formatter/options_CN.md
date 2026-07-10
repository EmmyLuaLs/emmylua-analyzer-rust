# EmmyLua Formatter 选项说明

[English](./options_EN.md)

本文档说明格式化器对外公开的配置分组、默认值以及各选项的预期影响。

## 配置文件发现规则

`luafmt` 和路径感知的库 API 都支持向上查找最近的配置文件：

- `.luafmt.toml`
- `luafmt.toml`

当输入来自 stdin 且没有源文件路径时，`luafmt` 会从当前工作目录开始向上查找最近的配置文件。

显式传入配置文件时，支持：

- TOML
- JSON
- YAML

## syntax

- `level`：Lua 语法等级，支持 `Lua51`、`Lua52`、`Lua53`、`Lua54`、`Lua55`、`LuaJIT`、`LuaJITExt`、`LuaJIT3`

默认值：

```toml
[syntax]
level = "Lua55"
```

说明：

- 这个选项决定格式化前使用哪一种 Lua 语法进行解析。
- 如果配置文件里没有写 `syntax.level`，默认按 `Lua55` 解析。
- CLI 的 `--level` 会覆盖配置文件里的 `syntax.level`。

## indent

- `kind`：`Space` 或 `Tab`
- `width`：缩进宽度

默认值：

```toml
[indent]
kind = "Space"
width = 4
```

## layout

- `max_line_width`：目标最大行宽
- `max_blank_lines`：保留的连续空行上限
- `table_expand`：`Never`、`Always`、`Auto`
- `call_args_expand`：`Never`、`Always`、`Auto`
- `func_params_expand`：`Never`、`Always`、`Auto`
- `prefer_call_args_layout_from_source`：是否优先保留显式多行调用参数列表的源码排版目标
- `prefer_table_layout_from_source`：是否优先保留显式多行纯数组 table 的源码排版目标
- `prefer_chain_break_on_statement_tail`：是否优先把语句尾部的 fluent chain 断成多行
- `prefer_binary_chain_operand_per_line`：为同一运算符的多元二元表达式链（3 个及以上操作数，例如一串 `and`/`or` 条件）提供“每个操作数一行”的候选布局

默认值：

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
prefer_binary_chain_operand_per_line = false
```

行为说明：

- `Auto` 表示允许格式化器在单行和多行候选之间进行比较。
- `prefer_call_args_layout_from_source = true` 只影响 `call_args_expand = "Auto"` 的调用参数列表。
- 如果调用参数列表在源码中本来就是多行，格式化器会优先保持展开，而不是再回流成更紧凑的布局。
- `prefer_table_layout_from_source = true` 只影响 `table_expand = "Auto"` 的 table constructor。
- 它只对纯数组 table 生效，也就是所有 field 都是不带 key 的位置值。
- 如果这样的 table 在源码中本来就是多行，格式化器会优先保持展开，而不是再回流成更紧凑的布局。
- 对于序列结构，格式化器在适用场景下会比较 fill、packed、aligned 和 one-per-line 等候选布局。
- 二元表达式链和语句表达式列表在总行数不变时，会优先选择更均衡的 packed 布局，以避免最后一行过短。
- `prefer_chain_break_on_statement_tail = true` 会影响语句中的最后一个直接表达式，包括独立调用语句，以及带 key 的 table field value；当它是一个足够长的 fluent chain 时，会优先改成链式断行。
- 这里的 chain head 定义为：`root` 加上前导命名空间/字段访问，再加上第一个调用段；但如果 `root` 本身已经是一个调用，则 head 就停在这个调用本身。例如 `Builder:new():add():add()` 的 head 是 `Builder:new()`，`ConsoleFormattingBuilder():setColor():build()` 的 head 是 `ConsoleFormattingBuilder()`，`vim.api.nvim_set_keymap(...)` 的 head 是整个 `vim.api.nvim_set_keymap(...)`。
- 这里的换行起点定义为：head 之后的第一个 continuation 段。也就是说，只有第一个调用之后仍然继续链下去的部分才会成为链式换行候选；纯命名空间限定调用不会因为这个选项被误判成 chain。
- `prefer_binary_chain_operand_per_line = true` 适用于由同一个运算符连接的 3 个及以上操作数，最常见的场景是 `if`/`while` 条件头里的一串 `and`/`or`。它只是新增一个候选布局；格式化器仍然会在 flat、fill、packed、one-operand-per-line 之间挑选行数最少且不超过 `max_line_width` 的那个，所以短链条或者用 fill/packed 就已经能放下的链条不受影响。
- 不开启这个选项时，如果整条链放不下，格式化器可能会转而在其中某个操作数内部断行（比如展开某个嵌套调用的参数列表），而不是在链自身的运算符处断行。开启后格式化器多了一个干净的备选方案：每个操作数单独一行，运算符放在每个续行的开头（`and`/`or` 在行首，与本项目现有二元表达式的前置运算符风格一致）。

## output

- `insert_final_newline`
- `trailing_comma`：`Never`、`Multiline`、`Always`
- `trailing_table_separator`：`Inherit`、`Never`、`Multiline`、`Always`
- `quote_style`：`Preserve`、`Double`、`Single`
- `single_arg_call_parens`：`Preserve`、`Always`、`Omit`
- `simple_lambda_single_line`：`Preserve`、`Always`、`Never`
- `end_of_line`：`LF` 或 `CRLF`

默认值：

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

行为说明：

- `trailing_comma` 是通用序列的尾逗号策略。
- `trailing_table_separator` 只覆盖 table 的尾部分隔符策略；设为 `Inherit` 时继承 `trailing_comma`。
- `quote_style` 只会在安全时重写普通短字符串；长字符串和其它字符串形式会保留原样。
- 引号重写基于原始 token 文本判断是否存在未转义的目标引号，并只做保持语义不变所需的最小分隔符转义调整。
- `single_arg_call_parens = "Omit"` 只会对 Lua 允许的单字符串参数调用和单 table 参数调用去掉括号。
- `simple_lambda_single_line = "Preserve"` 只会在输入本来就是单行简单 lambda 时保留单行。
- `simple_lambda_single_line = "Always"` 会在满足条件且不超出行宽时，将简单 lambda 收回成 `function(...) return expr end`。
- `simple_lambda_single_line = "Never"` 会关闭简单单行 lambda 快路径，始终把闭包体格式化为多行。

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

这些选项只控制 token 级别的空格，不直接决定更高层的布局是否换行。

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

默认值：

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

行为说明：

- statement 尾随注释对齐默认关闭。
- table、调用参数、函数参数中的尾随注释对齐是输入驱动的；只有源代码已经体现出额外空格的对齐意图时，才会启用。
- standalone comment 默认会打断对齐分组。
- table 字段尾随注释只在连续子组内部对齐，不会拖动整个表体。
- `space_after_comment_dash` 只会在普通 `--comment` 这类“前缀后完全没有空格”的情况下补一个空格；已有多个空格的注释会保留原样。

## emmy_doc

- `align_tag_columns`
- `align_declaration_tags`
- `align_reference_tags`
- `align_multiline_alias_descriptions`
- `space_between_tag_columns`
- `space_after_description_dash`
- `compact_type_or`

默认值：

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

当前已结构化处理的标签包括 `@param`、`@field`、`@return`、`@class`、`@alias`、`@type`、`@generic`、`@overload`。

- `align_multiline_alias_descriptions` 默认开启，用于把多行 `@alias` 块里 `--- | value # description` 的 `# description` 列对齐。
- `space_between_tag_columns` 控制 EmmyLua tag 行里 `---` 和 `@` 之间是否保留空格，例如 `--- @enum MyEnum` 和 `---@enum MyEnum` 的区别。当前默认值是 `false`，所以不额外配置时会输出 `---@tag`。
- `space_after_description_dash` 只影响普通 doc 描述行 `--- text` / `---text`，不影响 tag 行前缀。
- `compact_type_or` 控制 EmmyLua 联合类型里的 `|` 两侧是否保留空格，例如 `string | integer` 和 `string|integer`。默认值是 `false`。

## align

- `continuous_assign_statement`
- `table_field`

默认值：

```toml
[align]
continuous_assign_statement = false
table_field = true
```

行为说明：

- 连续赋值对齐默认关闭。
- 表字段对齐默认开启，但只有当输入在 `=` 后已经表现出额外空格的对齐意图时才会激活。

## 建议起步配置

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

## 完整默认配置

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

# EmmyLua Formatter 推荐配置方案

[English](./profiles_EN.md)

本文档给出几组适合常见团队风格的 formatter 推荐配置。它们不是内置模式，而是基于当前默认行为与布局策略整理出来的建议模板。

## 1. 保守默认方案

适用于历史风格混杂、注释较多、人工排版痕迹明显的代码库。

目标：

- 尽量减少意外重写
- 让对齐保持为输入驱动、按需启用
- 对序列结构继续使用 `Auto` 的宽度感知布局选择

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
align_across_standalone_comments = false

[align]
continuous_assign_statement = false
table_field = true
```

适用场景：

- 大型存量仓库
- 手工注释较多的游戏脚本仓库
- 希望稳定格式化、但不希望到处出现强对齐的团队

## 2. 团队统一方案

适用于希望统一格式化风格、但仍然保留保守注释策略的团队。

目标：

- 统一宽度和空格规则
- 保持注释可读性
- 让格式化器自动选择 flat、fill、packed 或 one-per-line 布局

```toml
[layout]
max_line_width = 88
table_expand = "Auto"
call_args_expand = "Auto"
func_params_expand = "Auto"

[output]
quote_style = "Double"
trailing_table_separator = "Multiline"
single_arg_call_parens = "Always"

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

适用场景：

- 使用 CI 格式检查的仓库
- 希望行宽和换行决策更可预测的团队
- 想使用 packed 布局，但不想让对齐规则过于激进的项目

## 3. 对齐敏感方案

只建议在代码库本身已经强依赖视觉对齐时使用。

目标：

- 尽量保留有意存在的表格与注释对齐
- 在已有视觉列的地方保持对齐结构

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

适用场景：

- 已经存在稳定视觉列风格的代码库
- 生成式或半生成式的脚本表数据
- 愿意认真审查对齐型 diff 的团队

## 说明

- 对 table、call arguments 和 parameters 来说，`Auto` 通常都是最合适的起点。
- formatter 现在已经为 binary chains 和 statement expression lists 提供了更均衡的 packed 布局，因此较窄的行宽也能保持相对紧凑的多行输出，而不必立刻退化成一项一行。
- 如果仓库里有很多脆弱的注释块，建议先从保守默认方案开始，观察 diff 质量后再逐步打开更强的对齐选项。

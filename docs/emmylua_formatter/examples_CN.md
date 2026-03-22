# EmmyLua Formatter 效果示例

[English](./examples_EN.md)

本页给出一组有代表性的前后对比例子，用来说明当前格式化器的布局策略。

## 能放一行时保持单行

Before:

```lua
local point={x=1,y=2}
```

After:

```lua
local point = { x = 1, y = 2 }
```

## 调用参数优先使用 Progressive Fill

Before:

```lua
some_function(first_arg, second_arg, third_arg, fourth_arg)
```

After:

```lua
some_function(
    first_arg, second_arg, third_arg,
    fourth_arg
)
```

这种布局会尽量保持紧凑，而不是一开始就退到一项一行。

## 二元表达式链的均衡 Packed 布局

Before:

```lua
local value = aaaa + bbbb + cccc + dddd + eeee + ffff
```

After:

```lua
local value = aaaa + bbbb
    + cccc + dddd
    + eeee + ffff
```

现在 binary chain 的候选评分会把真实的首行前缀宽度也算进去，因此像 `local value =` 这样的长锚点会正确影响候选选择。

## 语句表达式列表的均衡 Packed 布局

Before:

```lua
for key, value in first_long_expr, second_long_expr, third_long_expr, fourth_long_expr, fifth_long_expr do
    print(key, value)
end
```

After:

```lua
for key, value in first_long_expr,
    second_long_expr, third_long_expr,
    fourth_long_expr, fifth_long_expr do
    print(key, value)
end
```

这是 statement RHS 对 packed 布局的实际应用。第一项仍然贴在关键字所在行，后续项则按更均衡的方式打包。

## 必要时退到一段一行

Before:

```lua
builder:set_name(name):set_age(age):build()
```

After:

```lua
builder
    :set_name(name)
    :set_age(age)
    :build()
```

当更窄的布局明显更差时，格式化器仍然会退到一段一行。

## 注释对齐是输入驱动的

Before:

```lua
foo(
    alpha,  -- first
    beta   -- second
)
```

After:

```lua
foo(
    alpha, -- first
    beta   -- second
)
```

只有当输入已经体现出对齐意图时，格式化器才会对齐尾随注释；它不会在无关代码中主动制造宽对齐块。

# EmmyLua Formatter 效果示例

[English](./examples_EN.md)

本页按场景展示当前格式化器的典型布局结果。示例重点不是“所有代码都会变成同一种样子”，而是说明 formatter 会怎样在 flat、fill、packed、aligned 与 one-per-line 之间做选择。

## 1. 基础单行规整

### 能放一行时保持单行

Before:

```lua
local point={x=1,y=2}
```

After:

```lua
local point = { x = 1, y = 2 }
```

小而稳定的结构会优先保持单行，只做空格、逗号和分隔符的规范化。

## 2. 调用与参数序列

### 调用参数优先使用 Progressive Fill

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

### 嵌套调用只让外层换行，内层保持紧凑

Before:

```lua
cannotload("attempt to load a text chunk", load(read1(x), "modname", "b", {}))
```

After:

```lua
cannotload(
    "attempt to load a text chunk",
    load(read1(x), "modname", "b", {})
)
```

外层实参列表会根据行宽展开，但内部较短的子调用不会被连带打散。

### 函数参数中的尾随注释会被保留

Before:

```lua
local f = function(a -- first
, b)
    return a + b
end
```

After:

```lua
local f = function(a -- first
, b)
    return a + b
end
```

参数列表上的 inline comment 属于语义敏感区域，格式化器会优先保留原有结构。

## 3. 表构造

### 简短表保持紧凑

Before:

```lua
local t = { a = 1, b = 2, c = 3 }
```

After:

```lua
local t = { a = 1, b = 2, c = 3 }
```

### 关闭字段对齐后，Auto 模式使用渐进式换行

Before:

```lua
local t = { alpha, beta, gamma, delta }
```

After:

```lua
local t = {
    alpha, beta, gamma,
    delta
}
```

这类表不会因为换行就直接退成一项一行，而是先尝试更紧凑的分布。

### 嵌套表按结构决定是否展开

Before:

```lua
local t = { user = { name = "a", age = 1 }, enabled = true }
```

After:

```lua
local t = { user = { name = "a", age = 1 }, enabled = true }
```

格式化器不会因为“表里还有表”就机械地全部展开，而是先看整体形状和行宽。

## 4. 链式与表达式序列

### 二元表达式链使用更均衡的 Packed 布局

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

binary chain 的候选评分会把真实的首行前缀宽度算进去，因此像 local value = 这样的长锚点会参与布局选择。

### 语句表达式列表也会选择均衡 Packed 布局

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

第一项仍然贴在关键字所在行，后续项按更均衡的方式打包，而不是简单退到一项一行。

### 必要时退到一段一行

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

当 fill 或 packed 的结果明显更差时，格式化器仍然会退到更窄的一段一行布局。

## 5. 注释与保守策略

### 注释对齐是输入驱动的

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

### 语句头部的 inline comment 会保留在头部

Before:

```lua
if ready then -- inline comment
    work()
end
```

After:

```lua
if ready then -- inline comment
    work()
end
```

这类注释如果被移动进语句体，会改变阅读语义，因此 formatter 会保守处理。

# EmmyLua Formatter Examples

[中文文档](./examples_CN.md)

This page shows representative before-and-after examples for the formatter's current layout strategy.

## Flat When It Fits

Before:

```lua
local point={x=1,y=2}
```

After:

```lua
local point = { x = 1, y = 2 }
```

## Progressive Fill For Call Arguments

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

This keeps the argument list compact without immediately forcing one argument per line.

## Balanced Packed Layout For Binary Chains

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

The formatter now scores binary-chain candidates with the real first-line prefix width, so long anchors such as `local value =` influence candidate selection correctly.

## Balanced Packed Layout For Statement Expression Lists

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

This is the statement-level counterpart to packed binary chains. It keeps the first item attached to the keyword line and then packs later items in a balanced way.

## One Segment Per Line When Necessary

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

When narrower layouts are clearly worse, the formatter still falls back to one segment per line.

## Comment Alignment Is Input-Driven

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

The formatter aligns trailing comments only when the input already indicates alignment intent. It does not manufacture wide alignment blocks in unrelated code.

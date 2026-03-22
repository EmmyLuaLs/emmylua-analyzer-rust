# EmmyLua Formatter Examples

[中文文档](./examples_CN.md)

This page groups representative before-and-after examples by scenario. The point is not that every construct is formatted the same way, but that the formatter chooses between flat, fill, packed, aligned, and one-per-line layouts based on the rendered result.

## 1. Basic Flat Formatting

### Flat when it fits

Before:

```lua
local point={x=1,y=2}
```

After:

```lua
local point = { x = 1, y = 2 }
```

Small stable structures stay on one line, with spacing and separators normalized.

## 2. Calls And Parameter Lists

### Progressive fill for call arguments

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

### Outer calls may break while inner calls stay compact

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

The outer call expands because of width pressure, but short nested calls are not blown apart unnecessarily.

### Inline comments in parameter lists are preserved

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

Inline comments in parameter lists are treated conservatively because rewriting them can change how the signature reads.

## 3. Table Constructors

### Small tables stay compact

Before:

```lua
local t = { a = 1, b = 2, c = 3 }
```

After:

```lua
local t = { a = 1, b = 2, c = 3 }
```

### Auto mode uses progressive breaking when field alignment is off

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

The formatter tries a compact multi-line distribution before falling back to one item per line.

### Nested tables expand by shape, not by blanket rules

Before:

```lua
local t = { user = { name = "a", age = 1 }, enabled = true }
```

After:

```lua
local t = { user = { name = "a", age = 1 }, enabled = true }
```

Having a nested table is not enough on its own to force full expansion.

## 4. Chains And Expression Sequences

### Balanced packed layout for binary chains

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

Binary-chain candidates are scored with the real first-line prefix width, so long anchors such as local value = affect candidate selection.

### Statement expression lists also use balanced packed layouts

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

This keeps the first item attached to the keyword line and then packs later items more evenly.

### One segment per line when necessary

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

When fill or packed layouts are clearly worse, the formatter still falls back to one segment per line.

## 5. Comments And Conservative Preservation

### Comment alignment is input-driven

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

Trailing comments are aligned only when the input already signals alignment intent. The formatter does not manufacture wide alignment blocks across unrelated code.

### Inline comments on statement headers stay on the header

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

Moving this kind of comment into the body changes how the control flow reads, so the formatter preserves the header structure.

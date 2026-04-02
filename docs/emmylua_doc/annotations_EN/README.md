# EmmyLua Annotations Documentation Index

[中文版](../annotations_CN/README.md)

This directory contains detailed documentation and examples for all EmmyLua annotations.

## Syntax Notation Symbols

The following notation symbols are used in annotation syntax descriptions:

- `<name>` - Required placeholder that must be replaced with actual values
- `[value]` - Optional item, content within brackets is optional
- `[value...]` - Optional and repeatable item
- `value1 | value2` - Choice item, use either left or right value
- `<type[|type...]>` - Type expression, supports union types
- `[(modifier)]` - Optional modifier, such as `(exact)`, `(key)`, etc.
- `#` - Comment marker, followed by description text

## Core Annotations

### Type System
- [`@alias`](./alias.md) - Type alias definition
- [`@class`](./class.md) - Class definition
- [`@field`](./field.md) - Field definition
- [`@type`](./type.md) - Type declaration
- [`@enum`](./enum.md) - Enumeration definition
- [`@generic`](./generic.md) - Generic definition

### Function Annotations
- [`@param`](./param.md) - Parameter definition
- [`@return`](./return.md) - Return value definition
- `@return_overload` (see [`@return`](./return.md)) - Correlated return tuples
- [`@overload`](./overload.md) - Function overload
- [`@async`](./async.md) - Async function marker
- [`@nodiscard`](./nodiscard.md) - Non-discardable return value

### Type Operations
- [`@cast`](./cast.md) - Type casting

### Code Quality
- [`@deprecated`](./deprecated.md) - Deprecation marker
- [`@diagnostic`](./diagnostic.md) - Diagnostic control

### Metadata
- [`@meta`](./meta.md) - Metadata file
- [`@module`](./module.md) - Module declaration

### Other Annotations
- [`@operator`](./operator.md) - Operator overloading
- [`@see`](./see.md) - Reference to other symbols
- [`@source`](./source.md) - Source code reference
- [`@version`](./version.md) - Version requirements

## Usage Guide

### Basic Usage
Most annotations use the `---@` prefix and follow this format:
```lua
---@annotation_name parameters description
```

### Common Combinations
```lua
-- Class definition combination
---@class User
---@field id number User ID
---@field name string Username
---@field email string Email address

-- Function definition combination
---@param name string Username
---@param age number User age
---@return User User object
function createUser(name, age)
    return {id = generateId(), name = name, age = age}
end

-- Generic function combination
---@generic T
---@param items T[] List of items
---@param predicate fun(item: T): boolean Filter condition
---@return T[] Filtered list
function filter(items, predicate)
    -- Implementation code
end
```

### Best Practices

1. **Types First**: Define types before using them
2. **Progressive Enhancement**: Start with basic annotations, gradually add more complex ones
3. **Consistency**: Maintain consistent annotation style throughout the project
4. **Documentation**: Provide detailed descriptions for complex types and functions
5. **Test Validation**: Use type checking tools to validate annotation correctness

### Annotation Categories

#### Type Definition
- `@alias` - Simplify complex types
- `@class` - Define object structure
- `@enum` - Define enumeration values
- `@generic` - Define generic parameters

#### Function Related
- `@param` - Parameter types and descriptions
- `@return` - Return value types and descriptions
- `@overload` - Multiple calling methods
- `@async` - Async function marker

#### Code Quality
- `@deprecated` - Mark obsolete code
- `@diagnostic` - Control warning display
- `@nodiscard` - Force return value checking

#### Tool Support
- `@meta` - Type definition files
- `@cast` - Runtime type conversion

## Quick Reference

| Annotation | Purpose | Example |
|------------|---------|---------|
| `@alias` | Type alias | `---@alias StringOrNumber string \| number` |
| `@class` | Class definition | `---@class User` |
| `@field` | Field definition | `---@field name string` |
| `@param` | Parameter definition | `---@param name string` |
| `@return` | Return value definition | `---@return boolean` |
| `@return_overload` | Correlated return tuples | `---@return_overload true, T` |
| `@type` | Type declaration | `---@type string` |
| `@generic` | Generic definition | `---@generic T` |
| `@overload` | Function overload | `---@overload fun(x: number): number` |
| `@deprecated` | Deprecation marker | `---@deprecated Use new method instead` |
| `@cast` | Type casting | `---@cast value string` |

For more detailed information, please refer to the specific documentation for each annotation.

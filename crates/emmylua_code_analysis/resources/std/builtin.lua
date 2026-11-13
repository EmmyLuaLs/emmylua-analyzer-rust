--- @meta no-require

-- Copyright (c) 2018. tangzx(love.tangzx@qq.com)
--
-- Licensed under the Apache License, Version 2.0 (the "License"); you may not
-- use this file except in compliance with the License. You may obtain a copy of
-- the License at
--
-- http://www.apache.org/licenses/LICENSE-2.0
--
-- Unless required by applicable law or agreed to in writing, software
-- distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
-- WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
-- License for the specific language governing permissions and limitations under
-- the License.

-- Built-in Types

---
--- The type *nil* has one single value, **nil**, whose main property is to be
--- different from any other value; it usually represents the absence of a
--- useful value.
--- @class nil

---
--- The type *boolean* has two values, **false** and **true**. Both **nil** and
--- **false** make a condition false; any other value makes it true.
--- @class boolean

---
--- The type *number* uses two internal representations, or two subtypes, one
--- called *integer* and the other called *float*. Lua has explicit rules about
--- when each representation is used, but it also converts between them
--- automatically as needed. Therefore, the programmer may choose to mostly
--- ignore the difference between integers and floats or to assume complete
--- control over the representation of each number. Standard Lua uses 64-bit
--- integers and double-precision (64-bit) floats, but you can also compile
--- Lua so that it uses 32-bit integers and/or single-precision (32-bit)
--- floats. The option with 32 bits for both integers and floats is
--- particularly attractive for small machines and embedded systems. (See
--- macro LUA_32BITS in file luaconf.h.)
--- @class number

---
--- The subtype of *number* used for integer values (Lua 5.3+). In annotations
--- an integer is assignable to a number, but a number is not assignable to an
--- integer.
--- @class integer

---
--- The type *userdata* is provided to allow arbitrary C data to be stored in
--- Lua variables. A userdata value represents a block of raw memory. There
--- are two kinds of userdata: *full userdata*, which is an object with a block
--- of memory managed by Lua, and *light userdata*, which is simply a C pointer
--- value. Userdata has no predefined operations in Lua, except assignment
--- and identity test. By using *metatables*, the programmer can define
--- operations for full userdata values. Userdata values cannot be
--- created or modified in Lua, only through the C API. This guarantees the
--- integrity of data owned by the host program.
--- @class userdata

---
--- The type *lightuserdata* stores a bare C pointer. In Lua it has the same
--- predefined operations as *userdata* (assignment and identity comparison)
--- and is used, for example, by `debug.upvalueid`.
--- @class lightuserdata

---
--- The type *thread* represents independent threads of execution and it is
--- used to implement coroutines. Lua threads are not related to
--- operating-system threads. Lua supports coroutines on all systems, even those
--- that do not support threads natively.
--- @class thread

---
--- The type *table* implements associative arrays, that is, arrays that can
--- have as indices not only numbers, but any Lua value except **nil** and NaN.
--- (*Not a Number* is a special floating-point value used by the IEEE 754
--- standard to represent undefined or unrepresentable numerical results, such
--- as `0/0`.) Tables can be heterogeneous; that is, they can contain values of
--- all types (except **nil**). Any key with value **nil** is not considered
--- part of the table. Conversely, any key that is not part of a table has an
--- associated value **nil**.
---
--- Tables are the sole data-structuring mechanism in Lua; they can be used to
--- represent ordinary arrays, lists, symbol tables, sets, records, graphs,
--- trees, etc. To represent records, Lua uses the field name as an index. The
--- language supports this representation by providing `a.name` as syntactic
--- sugar for `a["name"]`. There are several convenient ways to create tables
--- in Lua.
---
--- Like indices, the values of table fields can be of any type. In particular,
--- because functions are first-class values, table fields can contain functions.
--- Thus tables can also carry *methods*.
---
--- The indexing of tables follows the definition of raw equality in the
--- language. The expressions `a[i]` and `a[j]` denote the same table element
--- if and only if `i` and `j` are raw equal (that is, equal without
--- metamethods). In particular, floats with integral values are equal to
--- their respective integers. To avoid ambiguities, any float with integral
--- value used as a key is converted to its respective integer. For instance,
--- if you write `a[2.0] = true`, the actual key inserted into the table will
--- be the integer `2`. (On the other hand, `2` and `"2"` are different Lua
--- values and therefore denote different table entries.)
--- @class table

---
--- The *any* type disables type checking: any value is assignable to `any`,
--- and `any` is assignable to any type.
--- @class any

---
--- The *void* type represents the absence of a return value. The analyzer
--- projects it to `nil`, so `fun(): void` behaves like `fun(): nil`.
--- @class void

---
--- The *unknown* type is used when a type cannot be inferred. Like `any`, it is
--- compatible with every type, but it also signals that inference failed.
--- Prefer `any` for values that are intentionally unchecked.
--- @class unknown

---
--- The *never* type is the bottom type: it is assignable to every type, but no
--- value is assignable to it. It is used mainly as the result of conditional
--- types such as `Exclude` (when the condition is true) and `Extract` (when the
--- condition is false).
--- @class never

---
--- The type of the implicit `self` parameter inside a method. It is replaced by
--- the owning class instance; using it outside a method is not meaningful.
--- @class self

--- Alias for `integer`, kept for C-style annotations.
--- @alias int integer

---
--- Reserved type name for namespace-qualified types. Namespaces are declared
--- with the `---@namespace` tag; this class only prevents `namespace<"...">`
--- from resolving to an unknown type.
--- @class namespace<T: string>

---
--- Reserved name for the primitive *function* type. `@type function` resolves
--- to the built-in function type, not to this class.
--- @class function

---
--- Removes `nil` from `T`; equivalent to `Exclude<T, nil>`.
--- @alias std.NotNull<T> Exclude<T, nil>

---
--- Adds `nil` to `T`; equivalent to `T | nil`.
--- @alias std.Nullable<T> T | nil

---
--- Marker type for the result tuple of `select`. The analyzer evaluates
--- `select` calls directly; the `unknown` body is a placeholder.
--- @alias std.Select<T, Num> unknown

---
--- Marker type for the multiple returns of `table.unpack`/`unpack`.
--- The analyzer expands all values of the tuple `T`; `Start` and `End` are
--- reserved and currently ignored.
--- @alias std.Unpack<T, Start, End> unknown

---
--- Marker type for `rawget(T, K)`: the value type stored at key `K` in `T`.
--- @alias std.RawGet<T, K> unknown

-- LuaLS compatibility aliases: allow annotations written for LuaLS to resolve
-- without the `std.` prefix.

--- @alias type std.type

--- @alias collectgarbage_opt std.collectgarbage_opt

--- @alias metatable std.metatable

---
--- A custom type guard. A function annotated with `---@return TypeGuard<T>`
--- narrows its argument to `T` in the truthy branch.
--- @alias TypeGuard<T> boolean

---
--- Marks a string argument that contains source code in the language named by
--- `T` (for example, `Language<"Lua">` for `load`/`loadstring` chunks). The
--- analyzer currently projects it to `string`, so embedded-language
--- highlighting is not applied.
--- @alias Language<T: string> string

---
--- Get the parameters of a function as a tuple
--- @alias Parameters<T extends function> T extends (fun(...: infer P): any) and P or never

---
--- Get the parameters of a constructor as a tuple
--- @alias ConstructorParameters<T> T extends new (fun(...: infer P): any) and P or never

--- Get the return type of a function type
--- @alias ReturnType<T extends function> T extends (fun(...: any): infer R) and R or any

---
--- Make all properties in T optional
--- @alias Partial<T> { [P in keyof T]?: T[P]; }

---
--- Exclude from T those types that are assignable to U
--- @alias Exclude<T, U> T extends U and never or T

---
--- Extract from T those types that are assignable to U
--- @alias Extract<T, U> T extends U and T or never

-- attribute

---
--- Base class for built-in attributes applied with the `---@[...]` syntax.
--- @class Attribute

---
--- Marks the declaration as deprecated. Receives an optional message.
---
--- Both the classic `---@deprecated` tag and the attribute form
--- `---@[deprecated("message")]` are supported.
--- @class deprecated: Attribute
--- @overload fun(message?: string)

---
--- Language Server Optimization Items.
---
--- Parameters:
--- - `delayed_definition`: Indicates that the type of the variable is determined by the first assignment.
---    Only valid for `local` declarations with no initial value.
--- @class lsp_optimization: Attribute
--- @overload fun(code: "delayed_definition")

---
--- Alias for an integer index field (for example, `---@field [1] string`).
--- The alias is shown in inlay hints and completion instead of the raw index.
---
--- Receives a string parameter for the alias name.
--- @class index_alias: Attribute
--- @overload fun(name: string)

---
--- This attribute must be applied to function parameters, immediately before
--- the corresponding `---@param` tag, and the parameter's type must be a string
--- template generic; it specifies the default constructor of the class produced
--- by the factory function.
---
--- Parameters:
--- - `name`: Name of the method used as the constructor (required).
--- - `root_class`: Reserved for LuaLS compatibility; accepted, but no implicit
---   inheritance is added by the current analyzer. Defaults to empty.
--- - `strip_self`: Whether the `self` parameter may be omitted when calling the
---   constructor. Defaults to `true`.
--- - `return_mode`: Constructor return strategy. `"self"` returns the class
---   instance, `"doc"` uses the constructor's documented return type, and
---   `"default"` prefers the documented return type and falls back to the
---   instance type. Defaults to `"default"`.
--- @class constructor: Attribute
--- @overload fun(name: string, root_class?: string, strip_self?: boolean, return_mode?: "self" | "doc" | "default")

---
--- Associates `getter` and `setter` methods with a field. Currently only
--- definition navigation is provided: jumping to the field also jumps to the
--- matching accessor methods on the field's class.
---
--- The attribute applies to the `@field` tag that follows it; an attribute
--- placed before `@class` applies to the first field.
---
--- Parameters:
--- - `convention`: Naming convention for the generated accessor names. Matches
---   the prefixes `get`/`set` with the field name and defaults to `camelCase`
---   (`_age` -> `getAge`, `setAge`; `first_name` -> `getFirstName`,
---   `setFirstName`). `"PascalCase"` produces `GetAge`/`SetAge`, `"snake_case"`
---   produces `get_age`/`set_age`.
--- - `getter`: Explicit getter method name. Takes precedence over `convention`.
--- - `setter`: Explicit setter method name. Takes precedence over `convention`.
--- @class field_accessor: Attribute
--- @overload fun(convention?: "camelCase" | "PascalCase" | "snake_case", getter?: string, setter?: string)

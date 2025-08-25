---@meta no-require

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

--- The type {lua}`nil` has one single value, *nil*, whose main property is to be
--- different from any other value; it usually represents the absence of a
--- useful value.
---@class nil

--- The type {lua}`boolean` has two values, *false* and *true*.
--- Both {lua}`nil` and *false* make a condition false; any other value
--- makes it true.
---@class boolean

--- Lua uses two internal representations for numbers: *integer* and *float*.
--- It has explicit rules about when each representation is used,
--- but it also converts between them automatically as needed.
---
--- EmmyLua, on the other hand, allows explicitly annotating which representation
--- is expected. The {lua}`number` type can contain both *integer* and *float*
--- values. The {lua}`integer` is a sub-type of {lua}`number`, and only allows
--- *integer* values.
---
--- ```{seealso}
--- Lua's [manual on value types].
--- ```
---
--- [manual on value types]: https://www.lua.org/manual/5.4/manual.html#2.1
---@class number

--- The type {lua}`integer` is a sub-type of {lua}`number` that only allows numbers
--- with *integer* representation.
---@class integer

--- The type {lua}`userdata` is provided to allow arbitrary C data to be stored in
--- Lua variables. A userdata value represents a block of raw memory. There
--- are two kinds of userdata: {lua}`userdata`, which is an object with a block
--- of memory managed by Lua, and {lua}`lightuserdata`, which is simply a C pointer
--- value.
---
--- ```{seealso}
--- Lua's [manual on value types].
--- ```
---
--- [manual on value types]: https://www.lua.org/manual/5.4/manual.html#2.1
---@class userdata

--- The type {lua}`lightuserdata` is a sub-type of {lua}`userdata` that only allows
--- values with *light userdata* representation.
---@class lightuserdata

--- The type {lua}`thread` represents independent threads of execution and it is
--- used to implement coroutines. Lua threads are not related to
--- operating-system threads. Lua supports coroutines on all systems, even those
--- that do not support threads natively.
---@class thread

--- The type *table* implements associative arrays, that is, arrays that can
--- have as indices not only numbers, but any Lua value except {lua}`nil` and
--- {lua}`NaN <number>`. (*Not a Number* is a special floating-point value used
--- by the IEEE 754 standard to represent undefined or unrepresentable numerical
--- results, such as `0/0`.)
---
--- While lua allows mixing types of keys and values in a table, EmmyLua has
--- an option to specify their exact types. Simply using type `table` creates
--- a heterogeneous table (equivalent to `table<unknown, unknown>`), while explicitly
--- providing key and value types creates a homogeneous table:
---
--- ```lua
--- --- @type table
--- local tableWithArbitraryData = {}
---
--- --- @type table<string, integer>
--- local tableWithStringKeysAndIntValues = {}
--- ```
---
--- You can also specify the exact shape of a table by using a *table literal*:
---
--- ```lua
--- --- @type { username: string, age: integer }
--- local User = { ... }
--- ```
---
--- ```{seealso}
--- Lua's [manual on value types].
--- ```
---
--- [manual on value types]: https://www.lua.org/manual/5.4/manual.html#2.1
---@class table<K, V>

--- The type {lua}`any` is compatible with any other type. That is, all types
--- can be converted to and from {lua}`any`.
---
--- This type is a way to bypass type checking system and explicitly tell EmmyLua
--- that you know what you're doing.
---
--- ```{tip}
--- Prefer using {lua}`unknown` instead of {lua}`any` to signal the need
--- to be careful and explicitly check value's contents.
--- ```
---@class any

--- The type {lua}`unknown` is similar to {lua}`any`,
--- but signifies a different intent.
---
--- While {lua}`any` is a way to say "I know what I'm doing", {lua}`unknown`
--- is a way to say "better check this value before using it".
---@class unknown

--- Void is an alias for {lua}`nil` used in some code bases. Prefer using
--- {lua}`nil` instead.
---@class void

--- A special type used with class methods. It can be thought of
--- as a generic parameter that matches type of the function's implicit argument
--- `self`. That is, when a function is called via colon notation
--- (i.e. `table:method()`), {lua}`self` is replaced with the type
--- of expression before the colon.
---
--- This is especially handy when dealing with inheritance.
--- Consider the following example:
---
--- ```lua
--- --- @class Base
--- local Base = {}
---
--- --- @return self
--- function Base:new()
---     return setmetatable({}, { __index=self })
--- end
---
--- --- @class Child: Base
--- local Child = setmetatable({}, { __index=Base })
---
--- local child = Child:new()
--- ```
---
--- Here, EmmyLua infers type of `child` to be `Child`, even though `new`
--- was defined in its base class. This is because `new` uses {lua}`self`
--- as its return type.
---@class self

---@alias int integer

---@class namespace<T: string>

---@class function

--- A type for {lua}`assert` function. Given a nullable type `T`
--- expands to a non-nullable version of `T`.
---
--- For example, if `T` is `string?`, then `std.NotNull<T>` will
--- be `string`.
---@alias std.NotNull<T> T - ?

--- An opposite of {lua}`std.NotNull`. Given a type `T`, expands
--- to a nullable version of `T`.
---
--- ```{tip}
--- Prefer using `T?` or `T | nil` instead.
--- ```
---@alias std.Nullable<T> T + ?

--- built-in type for Select function
---@alias std.Select<T, StartOrLen> unknown

--- A type for {lua}`table.unpack` function. Given a type `T`
--- and optional literal types `Start` and `End`, expands
--- to the type of expression `table.unpack(t, start, end)`.
---
--- Example:
---
--- ```lua
--- --- @generic T
--- --- @param list T
--- --- @return std.Unpack<T>
--- function customUnpack(list) end
---
--- local a, b, c = customUnpack({1, 2, 3})
--- ```
---
--- Here, `a`, `b` and `c` will be inferred as integers.
---@alias std.Unpack<T, Start, End> unknown

--- A type for {lua}`rawget` function. Given a type `T`
--- and a literal type `K`, expands to the type of expression
--- `rawget(t, k)`.
---
--- Example:
---
--- ```lua
--- --- @class Example
--- --- @field value integer
---
--- --- @type std.RawGet<Example, "value">
--- local value
--- ```
---
--- Here, `std.RawGet<Example, "value">` will be expanded to `integer`.
---@alias std.RawGet<T, K> unknown

--- A wrapper for matching literal types in generics.
---
--- By default, generics variables that match literal values
--- decay to values' base types:
---
--- ```lua
--- --- @generic T
--- --- @param x T
--- --- @return T
--- local function id(x)
---     return x
--- end
---
--- local original --- @type "literal"
---
--- local value = id(original)
--- ```
---
--- Here, type of `value` will be inferred as `string`
--- even though `original`'s type was `"literal"`.
---
--- We can prevent this behavior by wrapping generic pattern for `T`
--- into `std.ConstTpl<T>`:
---
--- ```lua
--- --- @generic T
--- --- @param x std.ConstTpl<T>
--- --- @return T
--- local function id(x)
---     return x
--- end
---
--- local original --- @type "literal"
---
--- local value = id(original)
--- ```
---
--- Here, type of `value` will be inferred as `"literal"`.
---@alias std.ConstTpl<T> unknown

--- compat luals

---@alias type std.type

---@alias collectgarbage_opt std.collectgarbage_opt

---@alias metatable std.metatable

---@alias TypeGuard<T> boolean

---@alias Language<T: string> string

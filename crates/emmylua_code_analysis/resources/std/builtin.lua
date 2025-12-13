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

---
--- The type *nil* has one single value, **nil**, whose main property is to be
--- different from any other value; it usually represents the absence of a
--- useful value.
---@class nil

---
--- The type *boolean* has two values, **false** and **true**. Both **nil** and
--- **false** make a condition false; any other value makes it true.
---@class boolean

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
---@class number

---@class integer

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
---@class userdata

---@class lightuserdata

---
--- The type *thread* represents independent threads of execution and it is
--- used to implement coroutines. Lua threads are not related to
--- operating-system threads. Lua supports coroutines on all systems, even those
--- that do not support threads natively.
---@class thread

---
--- The type *table* implements associative arrays, that is, arrays that can
--- have as indices not only numbers, but any Lua value except **nil** and NaN.
--- (*Not a Number* is a special floating-point value used by the IEEE 754
--- standard to represent undefined or unrepresentable numerical results, such
--- as `0/0`.) Tables can be heterogeneous; that is, they can contain values of
--- all types (except **nil**). Any key with value **nil** is not considered
--- part oft he table. Conversely, any key that is not part of a table has an
--- a ssociated value **nil**.
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
--- be the integer `2`. (On the other hand, 2 and "`2`" are different Lua
--- values and therefore denote different table entries.)
---@class table

---@class any

---@class void

---@class unknown

---@class never

---@class self

---@alias int integer

---@class namespace<T: string>

---@class function

---@alias std.NotNull<T> T - ?

---@alias std.Nullable<T> T + ?

---
--- built-in type for Select function
---@alias std.Select<T, StartOrLen> unknown

---
--- built-in type for Unpack function
---@alias std.Unpack<T, Start, End> unknown

---
--- built-in type for Rawget
---@alias std.RawGet<T, K> unknown

---
--- built-in type for generic template, for match integer const and true/false
---@alias std.ConstTpl<T> unknown

--- compact luals

---@alias type std.type

---@alias collectgarbage_opt std.collectgarbage_opt

---@alias metatable std.metatable

---@alias TypeGuard<T> boolean

---@alias Language<T: string> string

---
--- Get the parameters of a function as a tuple
---@alias Parameters<T extends function> T extends (fun(...: infer P): any) and P or never

---
--- Get the parameters of a constructor as a tuple
---@alias ConstructorParameters<T> T extends new (fun(...: infer P): any) and P or never

---
---@alias ReturnType<T extends function> T extends (fun(...: any): infer R) and R or any

---
--- Make all properties in T optional
---@alias Partial<T> { [P in keyof T]?: T[P]; }

--- attribute

---
--- Deprecated. Receives an optional message parameter.
---@attribute deprecated(message: string?)

---
--- Language Server Optimization Items.
---
--- Parameters:
--- - `check_table_field`: Skip the assign check for table fields. It is recommended to use this option for all large configuration tables.
--- - `delayed_definition`: Indicates that the type of the variable is determined by the first assignment.
---    Only valid for `local` declarations with no initial value.
---@attribute lsp_optimization(code: "check_table_field"|"delayed_definition")

---
--- Index field alias, will be displayed in `hint` and `completion`.
---
--- Receives a string parameter for the alias name.
---@attribute index_alias(name: string)

---
--- This attribute must be applied to function parameters, and the function parameter's type must be a string template generic,
--- used to specify the default constructor of a class.
---
--- Parameters:
--- - `name`: The name of the method as a constructor.
--- - `root_class`: Used to mark the root class, will implicitly inherit this class, such as `System.Object` in c#. Defaults to empty.
--- - `strip_self`: Whether the `self` parameter can be omitted when calling the constructor, defaults to `true`
--- - `return_self`: Whether the constructor is forced to return `self`, defaults to `true`
---@attribute constructor(name: string, root_class: string?, strip_self: boolean?, return_self: boolean?)

---
--- Associates `getter` and `setter` methods with a field. Currently provides only definition navigation functionality,
--- and the target methods must reside within the same class.
---
--- Parameters:
--- - `convention`: Naming convention, defaults to `camelCase`. Implicitly adds `get` and `set` prefixes. eg: `_age` -> `getAge`, `setAge`.
--- - `getter`: Getter method name. Takes precedence over `convention`.
--- - `setter`: Setter method name. Takes precedence over `convention`.
---@attribute field_accessor(convention: "camelCase"|"PascalCase"|"snake_case"|nil, getter: string?, setter: string?)

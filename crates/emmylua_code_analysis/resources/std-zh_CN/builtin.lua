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

--- *nil* 类型只有一个值 **nil**，其主要特性是与任何其他值都不同；
--- 它通常表示缺少有用的值。
--- @class nil

--- *boolean* 类型有两个值：**false** 和 **true**。**nil** 和 **false**
--- 都会使条件为假；任何其他值都会使条件为真。
--- @class boolean

--- **number** 类型在内部使用两种表示形式，或称为两种子类型，一种叫做
--- *integer*，另一种叫做 *float*。Lua 对于何时使用哪种表示形式有明确的规则，
--- 但也会在需要时自动进行转换。因此，程序员可以选择忽略整数和浮点数之间的差异，
--- 也可以完全控制每个数字的表示形式。标准 Lua 使用 64 位整数和双精度（64 位）
--- 浮点数，但你也可以将 Lua 编译为使用 32 位整数和/或单精度（32 位）浮点数。
--- 对于小型机器和嵌入式系统来说，同时使用 32 位整数和 32 位浮点数的选项特别有吸引力。
--- （参见 `luaconf.h` 文件中的宏 `LUA_32BITS`。）
--- @class number

--- @class integer

--- *userdata* 类型用于允许将任意 C 数据存储在 Lua 变量中。userdata 值
--- 表示一块原始内存。有两种 userdata：*full userdata*，是一个由 Lua
--- 管理内存块的对象；*light userdata*，它仅是一个 C 指针值。userdata 在
--- Lua 中没有预定义的操作，除了赋值和相等性测试。通过使用 *metatables*，程序员
--- 可以为 full userdata 值定义操作。userdata 值不能在 Lua 中创建或修改，
--- 只能通过 C API 进行。这保证了宿主程序所拥有数据的完整性。
--- @class userdata

--- @class lightuserdata

--- *thread* 类型表示独立的执行线程，用于实现协程。Lua 线程与操作系统
--- 线程无关。Lua 在所有系统上都支持协程，即使是那些原生不支持线程的系统。
--- @class thread

--- *table* 类型实现关联数组，即索引不仅可以是数字，还可以是除 **nil**
--- 和 NaN 之外的任何 Lua 值的数组。（*NaN* 是 IEEE 754 标准用于表示
--- 未定义或不可表示的数值结果的特殊浮点值，例如 `0/0`。）表可以是异构的；
--- 也就是说，它们可以包含所有类型的值（除了 **nil**）。值为 **nil** 的
--- 任何键都不被视为表的一部分。相反，任何不属于表的键其关联值都为 **nil**。
---
--- 表是 Lua 中唯一的数据结构；它们可以用来表示普通数组、列表、符号表、
--- 集合、记录、图、树等。为了表示记录，Lua 使用字段名作为索引。语言通过
--- 提供 `a.name` 作为 `a["name"]` 的语法糖来支持这种表示。
---
--- 与索引一样，表字段的值可以是任何类型。特别地，由于函数是一等公民，表
--- 字段可以包含函数。因此表也可以携带 *methods*。
---
--- 表的索引遵循语言中原始相等的定义。表达式 `a[i]` 和 `a[j]` 当且仅当
--- `i` 和 `j` 原始相等（即不使用元方法的相等）时表示相同的表元素。特别地，
--- 具有整数值的浮点数等于其对应的整数。为避免歧义，任何用作键的具有整数值
--- 的浮点数都会转换为其对应的整数。例如，如果你写 `a[2.0] = true`，
--- 插入表中的实际键将是整数 `2`。（另一方面，2 和 "`2`" 是不同的 Lua 值，
--- 因此表示不同的表条目。）
--- @class table

--- @class any

--- @class void

--- @class unknown

--- @class never

--- @class self

--- @alias int integer

--- @class namespace<T: string>

--- @class function

--- @alias std.NotNull<T> T -?

--- @alias std.Nullable<T> T +?

--- Select 函数的内置类型
--- @alias std.Select<T, StartOrLen> unknown

--- Unpack 函数的内置类型
--- @alias std.Unpack<T, Start, End> unknown

--- Rawget 的内置类型
--- @alias std.RawGet<T, K> unknown

--- compact luals

--- @alias type std.type

--- @alias collectgarbage_opt std.collectgarbage_opt

--- @alias metatable std.metatable

--- @alias TypeGuard<T> boolean

--- @alias Language<T: string> string

--- 以元组形式获取函数的参数
--- @alias Parameters<T extends function> T extends (fun(...: infer P): any) and P or never

--- 以元组形式获取构造函数的参数
--- @alias ConstructorParameters<T> T extends new (fun(...: infer P): any) and P or never

--- 获取函数类型的返回类型
--- @alias ReturnType<T extends function> T extends (fun(...: any): infer R) and R or any

--- 使 T 中的所有属性变为可选
--- @alias Partial<T> { [P in keyof T]?: T[P]; }

--- 排除 T 中可以赋值给 U 的类型
--- @alias Exclude<T, U> T extends U and never or T

--- 提取 T 中可以赋值给 U 的类型
--- @alias Extract<T, U> T extends U and T or never

--- attribute

--- @class Attribute

--- 标记为`已弃用`。接收一个可选的消息参数。
--- @class deprecated: Attribute
--- @overload fun(message?: string)

---
--- Language Server Optimization Items.
---
--- Parameters:
--- - `skip_table_fields_check`: Skip table field diagnostics. It is recommended to use this option for all large configuration tables.
--- - `delayed_definition`: Indicates that the type of the variable is determined by the first assignment.
---    Only valid for `local` declarations with no initial value.
--- @class lsp_optimization: Attribute
--- @overload fun(code: "skip_table_fields_check" | "delayed_definition")

---
--- Index field alias, will be displayed in `hint` and `completion`.
---
--- Receives a string parameter for the alias name.
--- @class index_alias: Attribute
--- @overload fun(name: string)

---
--- This attribute must be applied to function parameters, and the function parameter's type must be a string template generic,
--- used to specify the default constructor of a class.
---
--- Parameters:
--- - `name`: The name of the method as a constructor.
--- - `root_class`: Used to mark the root class, will implicitly inherit this class, such as `System.Object` in c#. Defaults to empty.
--- - `strip_self`: Whether the `self` parameter can be omitted when calling the constructor, defaults to `true`
--- - `return_mode`: Constructor return strategy. `"self"` forces `self`, `"doc"` uses the documented return type,
---                 and `"default"` prefers the documented return type and falls back to `self`.
---                 Defaults to `"default"`
--- @class constructor: Attribute
--- @overload fun(name: string, root_class?: string, strip_self?: boolean, return_mode?: "self" | "doc" | "default")

--- 将 `getter` 和 `setter` 方法与字段关联。目前仅提供定义跳转功能，
--- 且目标方法必须位于同一个类中。
---
--- ### 参数
---
--- - `convention`: 命名约定，默认为 `camelCase`。隐式添加 `get` 和 `set` 前缀，例如 `_age` -> `getAge`、`setAge`。
--- - `getter`: Getter 方法名。优先级高于 `convention`。
--- - `setter`: Setter 方法名。优先级高于 `convention`。
--- @class field_accessor: Attribute
--- @overload fun(convention?: "camelCase" | "PascalCase" | "snake_case", getter?: string, setter?: string)

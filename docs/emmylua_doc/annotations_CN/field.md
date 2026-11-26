# @field - 字段定义

为类定义字段，支持访问控制、可选性和索引签名。

## 语法

```lua
-- 具名字段
---@field [<访问控制>] <字段名>[?] <类型> [描述]

-- 索引签名字段
---@field [<访问控制>] [<键类型>] <值类型> [描述]
```

## 访问控制修饰符

- `public` - 公共字段（默认）
- `private` - 私有字段（仅类内部访问）
- `protected` - 受保护字段（类及子类可访问）
- `package` - 包内字段（同包内可访问）

## 示例

```lua
-- 基础字段定义
---@class User
---@field id number 用户ID
---@field name string 用户名
---@field email string 邮箱地址
---@field createdAt string 创建时间

-- 可选字段（使用 ? 标记）
---@class UserProfile
---@field avatar? string 头像URL（可选）
---@field bio? string 个人简介（可选）
---@field phone? string 电话号码（可选）

-- 访问控制示例
---@class BankAccount
---@field public accountNumber string 账户号码
---@field public balance number 账户余额
---@field private pin string PIN码
---@field protected accountType string 账户类型
---@field package internalId number 内部ID

-- 索引签名字段
---@class Configuration
---@field host string 主机地址
---@field port number 端口号
---@field [string] any 其他配置项（任意字符串键）

---@class ScoreBoard
---@field [string] number 学生姓名到分数的映射

---@class GenericContainer<T>
---@field [number] T 数组索引访问

-- 复杂字段类型
---@class APIResponse
---@field success boolean 请求是否成功
---@field data table | nil 响应数据
---@field error string | nil 错误信息
---@field meta {page: number, limit: number, total: number} 元数据

-- 函数类型字段
---@class EventEmitter
---@field listeners table<string, fun(...)> 事件监听器映射
---@field emit fun(self: EventEmitter, event: string, ...): boolean 发射事件
---@field on fun(self: EventEmitter, event: string, listener: fun(...)): nil 注册监听器

-- 嵌套类字段
---@class Address
---@field street string 街道
---@field city string 城市
---@field zipCode string 邮编

---@class Company
---@field name string 公司名称
---@field headquarters Address 总部地址
---@field branches Address[] 分部地址列表

-- 使用示例
---@type User
local user = {
    id = 1001,
    name = "张三",
    email = "zhangsan@example.com",
    createdAt = "2024-01-01"
}

---@type Configuration
local config = {
    host = "localhost",
    port = 8080,
    database = "myapp",  -- 通过索引签名支持
    cache = true         -- 通过索引签名支持
}

---@type ScoreBoard
local scores = {
    ["张三"] = 95,
    ["李四"] = 87,
    ["王五"] = 92
}
```

## 特性

1. **可选字段支持**
2. **访问控制**
3. **索引签名**
4. **复杂类型支持**
5. **嵌套结构**


## 不同的读取与写入类型

如果属性的 setter 接受的类型与 getter 返回的类型不同，可以声明一对互补字段：

```lua
---@class Burner
---@field (read) fuel? Fuel
---@field (write) fuel? string|Fuel

---@type Burner
local burner
burner.fuel = "coal" -- 按 string|Fuel|nil 检查赋值
local fuel = burner.fuel -- 即使刚写入字符串，读取类型仍为 Fuel|nil
```

不带方向修饰符的字段继续对读写使用同一个类型。每个方向可以分别设置可选标记，
也支持泛型。支持具名字段、字符串字面量键和整数字面量键；不支持带方向的索引签名，
也不建模 `__newindex`。在同一个字段上同时指定两种方向，或重复同一种修饰符，
会产生注解错误。重复声明同一方向的字段仍会产生重复字段错误（已有的函数重载规则仍然适用）。
重复的 `(write)` 函数声明允许赋入匹配任意一个 setter 签名的回调。
回调的上下文推断保留现有重载顺序；普通、不带方向修饰符的函数字段不受影响。

对于 `A|B` 这样的联合类型接收者，读取结果合并各分支的 getter 类型，赋值则接受
任一分支的 setter 类型，与普通联合类型字段的赋值行为一致。联合类型中的普通字段
提供其共同的读写类型。接收者收窄到某一分支后，可用的 setter 类型也随之收窄。

给成对声明的属性赋值会使先前的控制流类型收窄失效，恢复其声明的 getter 类型。
后续条件判断可以再次收窄该类型。嵌套赋值中的接收者始终按读取处理：
`object.property.child = x` 对 `property` 使用 getter 类型，对 `child` 使用 setter 类型。

这些注解描述类型，而不是访问权限。如果只声明一个方向，则读写都使用该类型，
不会引入只读或只写诊断。悬停提示和补全文档会同时显示一对字段的读写类型。

子类声明会整体覆盖继承的属性。因此，只声明一个方向的覆盖也会同时决定读写类型：

```lua
---@class Parent
---@field (read) value number
---@field (write) value string

---@class Child: Parent
---@field (read) value boolean

---@type Child
local child
child.value = true -- Child 接受 boolean；Parent 接受 string
local value = child.value -- boolean
```

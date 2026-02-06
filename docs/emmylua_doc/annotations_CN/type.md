# @type - 类型声明

为变量、表达式或对象指定具体的类型信息。

## 语法

```lua
---@type <类型表达式>
```

## 示例

```lua
-- 基础类型声明
---@type string
local userName = "张三"

---@type number
local userAge = 25

---@type boolean
local isActive = true

-- 联合类型
---@type string | number
local mixedValue = "可以是字符串或数字"

-- 交叉类型
---@type T & U
local mergedValue = getMergedValue()
-- 说明：这里的“交叉”指同时满足两种类型，因此会包含双方字段
-- 字段同时存在时，字段类型会变为两者的交叉；冲突时通常会变成 `never`（例如 {y:number} & {y:string} => y: never）
-- 需要“覆盖式合并”时，使用 Merge<T, U>（右侧覆盖；左侧覆盖可写为 Merge<U, T>）
-- 更适用于已定义字段的类或对象类型；开放/未知表会比较模糊

-- 可选类型
---@type string?
local optionalString = nil  -- 等价于 string | nil

-- 数组类型
---@type string[]
local nameList = {"张三", "李四", "王五"}

---@type number[]
local scores = {95, 87, 92, 88}

-- 复杂数组类型
---@type (string | number)[]
local mixedArray = {"张三", 25, "李四", 30}

-- 字典类型
---@type table<string, number>
local ageMap = {
    ["张三"] = 25,
    ["李四"] = 30,
    ["王五"] = 28
}

---@type table<number, string>
local idToName = {
    [1001] = "张三",
    [1002] = "李四",
    [1003] = "王五"
}

-- 元组类型
---@type [string, number, boolean]
local userInfo = {"张三", 25, true}

-- 表字面量类型
---@type {name: string, age: number, email: string}
local user = {
    name = "张三",
    age = 25,
    email = "zhangsan@example.com"
}

-- 嵌套表结构
---@type {user: {id: number, name: string}, permissions: string[]}
local userWithPermissions = {
    user = {id = 1001, name = "张三"},
    permissions = {"read", "write", "delete"}
}

-- 函数类型
---@type fun(x: number, y: number): number
local addFunction = function(x, y)
    return x + y
end

---@type fun(name: string, age: number): {name: string, age: number}
local createUser = function(name, age)
    return {name = name, age = age}
end

-- 异步函数类型
---@type async fun(url: string): string
local fetchData = async function(url)
    -- 异步获取数据
    return await httpGet(url)
end

-- 类类型
---@class User
---@field id number
---@field name string

---@type User
local currentUser = {
    id = 1001,
    name = "张三"
}

-- 类数组
---@type User[]
local userList = {
    {id = 1001, name = "张三"},
    {id = 1002, name = "李四"}
}

-- 泛型类型
---@class Container<T>
---@field items T[]

---@type Container<string>
local stringContainer = {
    items = {"hello", "world"}
}

---@type Container<number>
local numberContainer = {
    items = {1, 2, 3, 4, 5}
}

-- 复杂泛型组合
---@type table<string, Container<User>>
local userContainerMap = {
    ["admins"] = {items = {{id = 1, name = "管理员"}}},
    ["users"] = {items = {{id = 2, name = "普通用户"}}}
}

-- 枚举类型
---@alias Status 'active' | 'inactive' | 'pending'

---@type Status
local currentStatus = 'active'

-- 回调函数类型
---@type fun(error: string?, result: any?): nil
local callback = function(error, result)
    if error then
        print("错误:", error)
    else
        print("结果:", result)
    end
end

-- 事件处理器类型
---@type table<string, fun(...)>
local eventHandlers = {
    ["click"] = function(x, y)
        print("点击位置:", x, y)
    end,
    ["keypress"] = function(key)
        print("按键:", key)
    end
}

-- Promise类型
---@class Promise<T>
---@field then fun(self: Promise<T>, onResolve: fun(value: T), onReject?: fun(error: any))

---@type Promise<string>
local dataPromise = fetchUserDataAsync(1001)

-- 条件类型使用
---@type boolean
local isLoggedIn = checkLoginStatus()

---@type User | nil
local user = isLoggedIn and getCurrentUser() or nil

-- 索引签名类型
---@type {[string]: any}
local dynamicObject = {
    someKey = "someValue",
    anotherKey = 123,
    yetAnother = true
}

-- 只读类型（约定）
---@type {readonly name: string, readonly id: number}
local readonlyUser = {name = "张三", id = 1001}

-- 使用示例和类型检查
if user then
    -- 在这个块中，user的类型是 User（非nil）
    print("用户名:", user.name)
    print("用户ID:", user.id)
end

-- 类型断言使用
---@type string
local stringValue = tostring(mixedValue)  -- 确保转换为字符串

-- 循环中的类型使用
---@type string
for _, name in ipairs(nameList) do
    print("姓名:", name)  -- name被推断为string类型
end
```

## 特性

1. **基础类型支持**
2. **联合类型**
3. **数组和表类型**
4. **函数类型**
5. **泛型类型**
6. **条件类型**

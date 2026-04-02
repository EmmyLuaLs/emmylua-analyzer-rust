# @return - 返回值定义

定义函数的返回值类型和描述信息。

## 语法

```lua
-- 基础语法
---@return <类型> [变量名] [描述]

-- 带注释的语法
---@return <类型> [变量名] # 描述

-- 多返回值
---@return <类型1> [名称1] [描述1]
---@return <类型2> [名称2] [描述2]

-- 关联返回行（每一行代表一种返回元组）
---@return_overload <类型1>, <类型2>[, <类型3>...]
```

## 示例

```lua
-- 单一返回值
---@return string 用户名
function getCurrentUserName()
    return "张三"
end

-- 带变量名的返回值
---@return number result 计算结果
function calculate(x, y)
    return x + y
end

-- 多返回值
---@return boolean success 操作是否成功
---@return string message 结果消息
function validateInput(input)
    if input and input ~= "" then
        return true, "输入有效"
    else
        return false, "输入不能为空"
    end
end

-- 可选返回值（联合类型）
---@return User | nil 用户对象，如果未找到则返回nil
---@return string | nil 错误信息，如果成功则返回nil
function findUserById(id)
    local user = database:findUser(id)
    if user then
        return user, nil
    else
        return nil, "用户不存在"
    end
end

-- 复杂返回值类型
---@return {success: boolean, data: table[], count: number} 查询结果
function queryUsers(filters)
    local users = database:query("users", filters)
    return {
        success = true,
        data = users,
        count = #users
    }
end

-- 函数返回值
---@return fun(x: number): number 返回一个数学函数
function createMultiplier(factor)
    return function(x)
        return x * factor
    end
end

-- 泛型返回值
---@generic T
---@param value T
---@return T 输入值的副本
function clone(value)
    -- 深拷贝实现
    return deepCopy(value)
end

-- 可变返回值
---@return string ... 所有用户名
function getAllUserNames()
    return "张三", "李四", "王五"
end

-- 异步函数返回值
---@async
---@return Promise<string> 异步操作的Promise
function fetchUserDataAsync(userId)
    return Promise.new(function(resolve, reject)
        -- 异步获取数据
        setTimeout(function()
            if userId > 0 then
                resolve("用户数据")
            else
                reject("无效的用户ID")
            end
        end, 1000)
    end)
end

-- 条件返回值
---@param includeDetails boolean 是否包含详细信息
---@return string name 用户名
---@return number age 年龄
---@return string? email 邮箱（仅当includeDetails为true时返回）
function getUserInfo(includeDetails)
    local name, age = "张三", 25
    if includeDetails then
        return name, age, "zhangsan@example.com"
    else
        return name, age
    end
end

-- 错误处理模式
---@return boolean success 操作是否成功
---@return any result 成功时的结果数据
---@return string? error 失败时的错误信息
function safeOperation(data)
    local success, result = pcall(function()
        return processData(data)
    end)

    if success then
        return true, result, nil
    else
        return false, nil, result  -- result是错误信息
    end
end

-- 迭代器返回值
---@return fun(): number?, string? 迭代器函数
function iterateUsers()
    local users = {"张三", "李四", "王五"}
    local index = 0

    return function()
        index = index + 1
        if index <= #users then
            return index, users[index]
        end
        return nil, nil
    end
end

-- 使用示例
local name = getCurrentUserName()
local result = calculate(10, 20)

local success, message = validateInput("test")
if success then
    print("验证成功:", message)
end

local user, error = findUserById(123)
if user then
    print("找到用户:", user.name)
else
    print("错误:", error)
end

local queryResult = queryUsers({status = "active"})
print("查询到", queryResult.count, "个用户")

-- 使用迭代器
for id, userName in iterateUsers() do
    print(id, userName)
end
```

## 返回重载行（`@return_overload`）

`@return_overload` 用于定义“关联”的多返回值行。每一条注解代表一种可能的返回元组。
这对状态/结果模式（例如 `pcall` 风格代码）非常有用。

当多个局部变量来自同一次函数调用时，对某个返回槽位的条件判断
（真假判断、字面量相等判断，或 `type()` 守卫）会联动收窄同一返回行中的其他槽位。

```lua
---@generic T, E
---@param ok boolean
---@param success T
---@param failure E
---@return boolean
---@return T|E
---@return_overload true, T
---@return_overload false, E
local function pick(ok, success, failure)
    if ok then
        return true, success
    end
    return false, failure
end

local cond ---@type boolean
local ok, result = pick(cond, 1, "error")

if not ok then
    error(result) -- result: string
end

local value = result -- value: integer
```

`@return_overload` 同样支持泛型和可变尾部：

```lua
---@generic T, R
---@param f fun(...: T...): R...
---@param ... T...
---@return_overload true, R...
---@return_overload false, string
local function wrap(f, ...) end
```

你可以保留 `@return` 作为宽泛声明，再用 `@return_overload` 提供关联敏感的推断信息。

## 特性

1. **多返回值支持**
2. **可选返回值**
3. **泛型返回值**
4. **函数返回值**
5. **异步返回值**
6. **条件返回值**
7. **关联返回行收窄（`@return_overload`）**

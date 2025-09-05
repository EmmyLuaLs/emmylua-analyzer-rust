# @return - Return Value Definition

Define return value types and description information for functions.

## Syntax

```lua
-- Basic syntax
---@return <type> [variable_name] [description]

-- Syntax with comments
---@return <type> [variable_name] # description

-- Multiple return values
---@return <type1> [name1] [description1]
---@return <type2> [name2] [description2]
```

## Examples

```lua
-- Single return value
---@return string Username
function getCurrentUserName()
    return "John"
end

-- Return value with variable name
---@return number result Calculation result
function calculate(x, y)
    return x + y
end

-- Multiple return values
---@return boolean success Whether operation succeeded
---@return string message Result message
function validateInput(input)
    if input and input ~= "" then
        return true, "Input is valid"
    else
        return false, "Input cannot be empty"
    end
end

-- Optional return value (union type)
---@return User | nil User object, returns nil if not found
---@return string | nil Error message, returns nil if successful
function findUserById(id)
    local user = database:findUser(id)
    if user then
        return user, nil
    else
        return nil, "User not found"
    end
end

-- Complex return value type
---@return {success: boolean, data: table[], count: number} Query result
function queryUsers(filters)
    local users = database:query("users", filters)
    return {
        success = true,
        data = users,
        count = #users
    }
end

-- Function return value
---@return fun(x: number): number Returns a mathematical function
function createMultiplier(factor)
    return function(x)
        return x * factor
    end
end

-- Generic return value
---@generic T
---@param value T
---@return T Copy of input value
function clone(value)
    -- Deep copy implementation
    return deepCopy(value)
end

-- Variadic return values
---@return string ... All usernames
function getAllUserNames()
    return "John", "Jane", "Bob"
end

-- Async function return value
---@async
---@return Promise<string> Promise of async operation
function fetchUserDataAsync(userId)
    return Promise.new(function(resolve, reject)
        -- Fetch data asynchronously
        setTimeout(function()
            if userId > 0 then
                resolve("User data")
            else
                reject("Invalid user ID")
            end
        end, 1000)
    end)
end

-- Conditional return values
---@param includeDetails boolean Whether to include detailed information
---@return string name Username
---@return number age Age
---@return string? email Email (only returned when includeDetails is true)
function getUserInfo(includeDetails)
    local name, age = "John", 25
    if includeDetails then
        return name, age, "john@example.com"
    else
        return name, age
    end
end

-- Error handling pattern
---@return boolean success Whether operation succeeded
---@return any result Result data on success
---@return string? error Error message on failure
function safeOperation(data)
    local success, result = pcall(function()
        return processData(data)
    end)

    if success then
        return true, result, nil
    else
        return false, nil, result  -- result is error message
    end
end

-- Iterator return value
---@return fun(): number?, string? Iterator function
function iterateUsers()
    local users = {"John", "Jane", "Bob"}
    local index = 0

    return function()
        index = index + 1
        if index <= #users then
            return index, users[index]
        end
        return nil, nil
    end
end

-- Usage examples
local name = getCurrentUserName()
local result = calculate(10, 20)

local success, message = validateInput("test")
if success then
    print("Validation successful:", message)
end

local user, error = findUserById(123)
if user then
    print("Found user:", user.name)
else
    print("Error:", error)
end

local queryResult = queryUsers({status = "active"})
print("Found", queryResult.count, "users")

-- Using iterator
for id, userName in iterateUsers() do
    print(id, userName)
end
```

## Features

1. **Multiple return value support**
2. **Optional return values**
3. **Generic return values**
4. **Function return values**
5. **Async return values**
6. **Conditional return values**

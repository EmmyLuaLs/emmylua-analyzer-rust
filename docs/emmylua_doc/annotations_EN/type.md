# @type - Type Declaration

Specify concrete type information for variables, expressions, or objects.

## Syntax

```lua
---@type <type_expression>
```

## Examples

```lua
-- Basic type declarations
---@type string
local userName = "John"

---@type number
local userAge = 25

---@type boolean
local isActive = true

-- Union types
---@type string | number
local mixedValue = "Can be string or number"

-- Intersection types
---@type T & U
local mergedValue = getMergedValue()
-- Values must satisfy both types, so fields from both are included.
-- If a field exists on both sides, its type becomes the intersection of the two types.
-- Conflicting field types often collapse to `never` (e.g. {y:number} & {y:string} => y: never).
-- For overwrite-style merges, use Merge<T, U> (right wins; use Merge<U, T> for left wins).
-- Best used with classes or object types that define fields; open/unknown tables stay vague

-- Optional types
---@type string?
local optionalString = nil  -- Equivalent to string | nil

-- Array types
---@type string[]
local nameList = {"John", "Jane", "Bob"}

---@type number[]
local scores = {95, 87, 92, 88}

-- Complex array types
---@type (string | number)[]
local mixedArray = {"John", 25, "Jane", 30}

-- Dictionary types
---@type table<string, number>
local ageMap = {
    ["John"] = 25,
    ["Jane"] = 30,
    ["Bob"] = 28
}

---@type table<number, string>
local idToName = {
    [1001] = "John",
    [1002] = "Jane",
    [1003] = "Bob"
}

-- Tuple types
---@type [string, number, boolean]
local userInfo = {"John", 25, true}

-- Table literal types
---@type {name: string, age: number, email: string}
local user = {
    name = "John",
    age = 25,
    email = "john@example.com"
}

-- Nested table structures
---@type {user: {id: number, name: string}, permissions: string[]}
local userWithPermissions = {
    user = {id = 1001, name = "John"},
    permissions = {"read", "write", "delete"}
}

-- Function types
---@type fun(x: number, y: number): number
local addFunction = function(x, y)
    return x + y
end

---@type fun(name: string, age: number): {name: string, age: number}
local createUser = function(name, age)
    return {name = name, age = age}
end

-- Async function types
---@type async fun(url: string): string
local fetchData = async function(url)
    -- Fetch data asynchronously
    return await httpGet(url)
end

-- Class types
---@class User
---@field id number
---@field name string

---@type User
local currentUser = {
    id = 1001,
    name = "John"
}

-- Class arrays
---@type User[]
local userList = {
    {id = 1001, name = "John"},
    {id = 1002, name = "Jane"}
}

-- Generic types
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

-- Complex generic combinations
---@type table<string, Container<User>>
local userContainerMap = {
    ["admins"] = {items = {{id = 1, name = "Admin"}}},
    ["users"] = {items = {{id = 2, name = "Regular User"}}}
}

-- Enumeration types
---@alias Status 'active' | 'inactive' | 'pending'

---@type Status
local currentStatus = 'active'

-- Callback function types
---@type fun(error: string?, result: any?): nil
local callback = function(error, result)
    if error then
        print("Error:", error)
    else
        print("Result:", result)
    end
end

-- Event handler types
---@type table<string, fun(...)>
local eventHandlers = {
    ["click"] = function(x, y)
        print("Click position:", x, y)
    end,
    ["keypress"] = function(key)
        print("Key pressed:", key)
    end
}

-- Promise types
---@class Promise<T>
---@field then fun(self: Promise<T>, onResolve: fun(value: T), onReject?: fun(error: any))

---@type Promise<string>
local dataPromise = fetchUserDataAsync(1001)

-- Conditional type usage
---@type boolean
local isLoggedIn = checkLoginStatus()

---@type User | nil
local user = isLoggedIn and getCurrentUser() or nil

-- Index signature types
---@type {[string]: any}
local dynamicObject = {
    someKey = "someValue",
    anotherKey = 123,
    yetAnother = true
}

-- Readonly types (by convention)
---@type {readonly name: string, readonly id: number}
local readonlyUser = {name = "John", id = 1001}

-- Usage examples and type checking
if user then
    -- In this block, user's type is User (non-nil)
    print("Username:", user.name)
    print("User ID:", user.id)
end

-- Type assertion usage
---@type string
local stringValue = tostring(mixedValue)  -- Ensure conversion to string

-- Type usage in loops
---@type string
for _, name in ipairs(nameList) do
    print("Name:", name)  -- name is inferred as string type
end
```

## Features

1. **Basic type support**
2. **Union types**
3. **Array and table types**
4. **Function types**
5. **Generic types**
6. **Conditional types**

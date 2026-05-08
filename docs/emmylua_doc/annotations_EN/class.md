# @class - Class Definition

Define classes or interfaces, supporting inheritance, field definitions, and access control.

## Syntax

```lua
-- Basic class definition
---@class <class_name>[: <parent_class1>[, <parent_class2>...]]

-- Exact class definition (prohibits dynamic field addition)
---@class (exact) <class_name>[: <parent_class>...]

-- Partial class definition (allows extending existing classes)
---@class (partial) <class_name>

---@class <generic_class_name><T1, T2, ...>[: <parent_class>...]
```

## Examples

```lua
-- Basic class definition
---@class Animal
---@field name string Animal name
---@field species string Species
---@field age number Age
local Animal = {}

---@param name string
---@param species string
---@param age number
function Animal.new(name, species, age)
    return setmetatable({
        name = name,
        species = species,
        age = age
    }, {__index = Animal})
end

function Animal:speak()
    print(self.name .. " makes a sound")
end

-- Inheritance example
---@class Dog : Animal
---@field breed string Breed
---@field isVaccinated boolean Whether vaccinated
local Dog = setmetatable({}, {__index = Animal})

function Dog:speak()
    print(self.name .. " barks: Woof!")
end

---@param name string
---@param breed string
---@param age number
---@return Dog
function Dog.new(name, breed, age)
    local self = Animal.new(name, "Canine", age)
    self.breed = breed
    self.isVaccinated = false
    return setmetatable(self, {__index = Dog})
end

-- Multiple inheritance example
---@class Flyable
---@field maxAltitude number Maximum flight altitude

---@class Swimmable
---@field maxDepth number Maximum diving depth

---@class Duck : Animal, Flyable, Swimmable
---@field featherColor string Feather color
local Duck = {}

-- Exact class definition example (cannot add fields dynamically)
---@class (exact) Point
---@field x number
---@field y number
local Point = {}

-- Partial class definition example (extends existing class)
---@class (partial) Animal
---@field weight number Weight

-- Generic class example
---@class Container<T>
---@field private items T[] Stored items
---@field capacity number Capacity
local Container = {}

---@generic T
---@param capacity number
---@return Container<T>
function Container.new(capacity)
    return {items = {}, capacity = capacity}
end

---@param item T
function Container:add(item)
    if #self.items < self.capacity then
        table.insert(self.items, item)
    end
end

-- Usage example
---@type Dog
local myDog = Dog.new("Buddy", "Golden Retriever", 3)
myDog:speak()

---@type Container<string>
local stringContainer = Container.new(10)
stringContainer:add("Hello")
```

## Features

1. **Single and multiple inheritance**
2. **Exact type control**
3. **Generic class support**
4. **Field access control**
5. **Type safety checking**

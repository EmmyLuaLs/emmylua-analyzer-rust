# @class - 类定义

定义类或接口，支持继承、字段定义和访问控制。

## 语法

```lua
-- 基础类定义
---@class <类名>[: <父类1>[, <父类2>...]]

-- 精确类定义（禁止动态添加字段）
---@class (exact) <类名>[: <父类>...]

-- 部分类定义（允许扩展现有类）
---@class (partial) <类名>

---@class <generic_class_name><T1, T2, ...>[: <parent_class>...]
```

## 示例

```lua
-- 基础类定义
---@class Animal
---@field name string 动物名称
---@field species string 物种
---@field age number 年龄
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

-- 继承示例
---@class Dog : Animal
---@field breed string 品种
---@field isVaccinated boolean 是否已接种疫苗
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

-- 多重继承示例
---@class Flyable
---@field maxAltitude number 最大飞行高度

---@class Swimmable
---@field maxDepth number 最大潜水深度

---@class Duck : Animal, Flyable, Swimmable
---@field featherColor string 羽毛颜色
local Duck = {}

-- 精确类定义示例（不能动态添加字段）
---@class (exact) Point
---@field x number
---@field y number
local Point = {}

-- 部分类定义示例（扩展已有类）
---@class (partial) Animal
---@field weight number 体重

-- 泛型类示例
---@class Container<T>
---@field private items T[] 存储的项目
---@field capacity number 容量
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

-- 使用示例
---@type Dog
local myDog = Dog.new("Buddy", "Golden Retriever", 3)
myDog:speak()

---@type Container<string>
local stringContainer = Container.new(10)
stringContainer:add("Hello")
```

## 特性

1. **单一继承和多重继承**
2. **精确类型控制**
3. **泛型类支持**
4. **字段访问控制**
5. **类型安全检查**

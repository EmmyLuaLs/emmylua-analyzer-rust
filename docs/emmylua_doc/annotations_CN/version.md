# @version - 版本要求

指定代码的Lua版本要求。

## 语法

```lua
---@version [<|>]<版本号>
```

## 示例

```lua
-- 最低版本要求
---@version >5.1
function modernFeature()
    -- 需要Lua 5.2+的特性
    local function closure()
        -- 使用了5.2+的特性
    end
end

-- 特定版本
---@version 5.4
function lua54Feature()
    -- 仅在Lua 5.4中可用的特性
    local x <const> = 10  -- 常量变量
end

-- 多版本兼容
---@version 5.1,5.2,5.3
function compatibleFeature()
    -- 兼容多个版本的代码
end

-- 版本范围
---@version >5.2,<5.5
function rangeCompatible()
    -- 5.2到5.4兼容
end

-- JIT版本
---@version JIT
function jitOptimized()
    -- 针对LuaJIT优化的代码
end
```

## 特性

1. **版本检查**
2. **兼容性标记**
3. **特性标识**
4. **工具支持**
5. **文档生成**

# @version - Version Requirement

Specifies the required Lua version for the code.

## Syntax

```lua
---@version [<|>]<version>
```

## Examples

```lua
-- Minimum version requirement
---@version >5.1
function modernFeature()
    -- requires features from Lua 5.2+
    local function closure()
        -- uses 5.2+ features
    end
end

-- Specific version
---@version 5.4
function lua54Feature()
    -- features only available in Lua 5.4
    local x <const> = 10  -- constant variable
end

-- Multi-version compatibility
---@version 5.1,5.2,5.3
function compatibleFeature()
    -- code compatible with multiple versions
end

-- Version range
---@version >5.2,<5.5
function rangeCompatible()
    -- compatible from 5.2 to 5.4
end

-- JIT version
---@version JIT
function jitOptimized()
    -- optimized for LuaJIT
end
```

## Features

1. **Version checking**
2. **Compatibility tagging**
3. **Feature identification**
4. **Tool support**
5. **Documentation generation**

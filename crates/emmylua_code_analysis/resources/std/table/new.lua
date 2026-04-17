--- Copy from Lua Sumneko Lua
--- @meta table.new
--- @version JIT

--- @version JIT
---
--- This creates a pre-sized table, just like the C API equivalent `lua_createtable()`. This is useful for big tables if the final table size is known and automatic table resizing is too expensive. `narray` parameter specifies the number of array-like items, and `nhash` parameter specifies the number of hash-like items. The function needs to be required before use.
--- ```lua
---    require("table.new")
--- ```
---
---
--- [View documents](command:extension.lua.doc?["en-us/54/manual.html/pdf-table.new"])
---
--- @param narray integer
--- @param nhash  integer
--- @return table
local function new(narray, nhash) end

return new

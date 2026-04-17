--- Copy from Lua Sumneko Lua
--- @meta table.clear
--- @version JIT

--- @version JIT
---
--- This clears all keys and values from a table, but preserves the allocated array/hash sizes. This is useful when a table, which is linked from multiple places, needs to be cleared and/or when recycling a table for use by the same context. This avoids managing backlinks, saves an allocation and the overhead of incremental array/hash part growth. The function needs to be required before use.
--- ```lua
---    require("table.clear").
--- ```
--- Please note this function is meant for very specific situations. In most cases it's better to replace the (usually single) link with a new table and let the GC do its work.
---
---
--- [View documents](command:extension.lua.doc?["en-us/54/manual.html/pdf-table.clear"])
---
--- @param tab table
local function clear(tab) end

return clear

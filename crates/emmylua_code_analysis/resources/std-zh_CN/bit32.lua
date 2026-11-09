--- Copy from Lua Sumneko Lua

--- @meta bit32
--- @version 5.2

--- @version 5.2
--- [查看文档](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32"])
--- @class bit32lib
bit32 = {}

--- 返回 `x` 向右算术位移 `disp` 位的结果。`disp` 为负时向左位移。
---
--- 这是算术位移操作：左侧空位用 `x` 的最高位填充，右侧空位用 `0` 填充。
---
--- [查看文档](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.arshift"])
--- @param x    integer
--- @param disp integer
--- @return integer
--- @nodiscard
function bit32.arshift(x, disp) end

--- 返回其操作数的按位与（AND）结果。
---
--- [查看文档](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.band"])
--- @return integer
--- @nodiscard
function bit32.band(...) end

--- 返回 `x` 的按位取反结果。
---
--- ```lua
--- assert(bit32.bnot(x) ==
--- (-1 - x) % 2^32)
--- ```
---
--- [查看文档](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.bnot"])
--- @param x integer
--- @return integer
--- @nodiscard
function bit32.bnot(x) end

--- 返回其操作数的按位或（OR）结果。
---
--- [查看文档](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.bor"])
--- @return integer
--- @nodiscard
function bit32.bor(...) end

--- 返回一个布尔值，表示其操作数的按位与（AND）结果是否不为零。
---
--- [查看文档](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.btest"])
--- @return boolean
--- @nodiscard
function bit32.btest(...) end

--- 返回其操作数的按位异或（XOR）结果。
---
--- [查看文档](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.bxor"])
--- @return integer
--- @nodiscard
function bit32.bxor(...) end

--- 返回由 `n` 的第 `field` 位到第 `field + width - 1` 位构成的无符号数。
---
--- [查看文档](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.extract"])
--- @param n      integer
--- @param field  integer
--- @param width? integer
--- @return integer
--- @nodiscard
function bit32.extract(n, field, width) end

--- 返回 `n` 的一个副本，并将第 `field` 位到第 `field + width - 1` 位替换为 `v` 的值。
---
--- [查看文档](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.replace"])
--- @param n      integer
--- @param v      integer
--- @param field  integer
--- @param width? integer
--- @nodiscard
function bit32.replace(n, v, field, width) end

--- 返回 `x` 向左旋转 `disp` 位的结果。`disp` 为负时向右旋转。
---
--- [查看文档](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.lrotate"])
--- @param x     integer
--- @param distp integer
--- @return integer
--- @nodiscard
function bit32.lrotate(x, distp) end

--- 返回 `x` 向左位移 `disp` 位的结果。`disp` 为负时向右位移。无论方向如何，空位都用 `0` 填充。
---
--- ```lua
--- assert(bit32.lshift(b, disp) ==
--- (b * 2^disp) % 2^32)
--- ```
---
--- [查看文档](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.lshift"])
--- @param x     integer
--- @param distp integer
--- @return integer
--- @nodiscard
function bit32.lshift(x, distp) end

--- 返回 `x` 向右旋转 `disp` 位的结果。`disp` 为负时向左旋转。
---
--- [查看文档](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.rrotate"])
--- @param x     integer
--- @param distp integer
--- @return integer
--- @nodiscard
function bit32.rrotate(x, distp) end

--- 返回 `x` 向右位移 `disp` 位的结果。`disp` 为负时向左位移。无论方向如何，空位都用 `0` 填充。
---
--- ```lua
--- assert(bit32.rshift(b, disp) ==
--- math.floor(b % 2^32 / 2^disp))
--- ```
---
--- [查看文档](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.rshift"])
--- @param x     integer
--- @param distp integer
--- @return integer
--- @nodiscard
function bit32.rshift(x, distp) end

return bit32

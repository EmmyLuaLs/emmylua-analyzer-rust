--- Copy from Lua Sumneko Lua
--- @meta bit
--- @version JIT

--- LuaJIT 内置的 32 位按位运算库。
---
--- 所有参数都会被归一化为有符号 32 位整数，所有结果也都是有符号 32 位整数。参见 [Lua BitOp API](https://bitop.luajit.org/api.html)。
--- @class bitlib
bit = {}

--- 将数字 `x` 归一化到其他函数使用的数值范围。
--- @param x number
--- @return integer
--- @nodiscard
function bit.tobit(x) end

--- 将 `x` 转换为十六进制字符串。
---
--- `n` 的绝对值表示十六进制数字的位数（默认为 `8`）。`n` 为正数时使用小写字母，为负数时使用大写字母。只使用最低的 `4 * |n|` 位。
--- @param x number
--- @param n? integer
--- @return string
--- @nodiscard
function bit.tohex(x, n) end

--- 返回 `x` 的按位取反结果。
--- @param x number
--- @return integer
--- @nodiscard
function bit.bnot(x) end

--- 返回所有参数的按位或结果。
--- @param x number
--- @param ... number
--- @return integer
--- @nodiscard
function bit.bor(x, ...) end

--- 返回所有参数的按位与结果。
--- @param x number
--- @param ... number
--- @return integer
--- @nodiscard
function bit.band(x, ...) end

--- 返回所有参数的按位异或结果。
--- @param x number
--- @param ... number
--- @return integer
--- @nodiscard
function bit.bxor(x, ...) end

--- 返回 `x` 逻辑左移 `n` 位的结果。
---
--- 只使用 `n` 的最低 5 位，因此移位量为 `0..31`。
--- @param x number
--- @param n integer
--- @return integer
--- @nodiscard
function bit.lshift(x, n) end

--- 返回 `x` 逻辑右移 `n` 位的结果。
---
--- 空出的位用 0 填充。只使用 `n` 的最低 5 位。
--- @param x number
--- @param n integer
--- @return integer
--- @nodiscard
function bit.rshift(x, n) end

--- 返回 `x` 算术右移 `n` 位的结果。
---
--- 空出的位用符号位填充。只使用 `n` 的最低 5 位。
--- @param x number
--- @param n integer
--- @return integer
--- @nodiscard
function bit.arshift(x, n) end

--- 返回 `x` 循环左移 `n` 位的结果。
---
--- 只使用 `n` 的最低 5 位。
--- @param x number
--- @param n integer
--- @return integer
--- @nodiscard
function bit.rol(x, n) end

--- 返回 `x` 循环右移 `n` 位的结果。
---
--- 只使用 `n` 的最低 5 位。
--- @param x number
--- @param n integer
--- @return integer
--- @nodiscard
function bit.ror(x, n) end

--- 交换 `x` 的字节顺序（小端与大端互换）。
--- @param x number
--- @return integer
--- @nodiscard
function bit.bswap(x) end

return bit

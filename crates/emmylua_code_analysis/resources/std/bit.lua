--- Copy from Lua Sumneko Lua
--- @meta bit
--- @version JIT

--- LuaJIT's built-in 32-bit bitwise operations.
---
--- All arguments are normalized to signed 32-bit integers and all results are
--- signed 32-bit integers. See the
--- [Lua BitOp API](https://bitop.luajit.org/api.html).
---
--- @class bitlib
bit = {}

--- Normalizes `x` to the numeric range used by the other functions.
--- @param x number
--- @return integer
--- @nodiscard
function bit.tobit(x) end

--- Converts `x` to a hexadecimal string.
---
--- The absolute value of `n` is the number of hex digits (default `8`).
--- A positive `n` produces lowercase digits, a negative `n` produces uppercase
--- digits. Only the lowest `4 * |n|` bits are used.
--- @param x number
--- @param n? integer
--- @return string
--- @nodiscard
function bit.tohex(x, n) end

--- Returns the bitwise NOT of `x`.
--- @param x number
--- @return integer
--- @nodiscard
function bit.bnot(x) end

--- Returns the bitwise OR of all arguments.
--- @param x number
--- @param ... number
--- @return integer
--- @nodiscard
function bit.bor(x, ...) end

--- Returns the bitwise AND of all arguments.
--- @param x number
--- @param ... number
--- @return integer
--- @nodiscard
function bit.band(x, ...) end

--- Returns the bitwise XOR of all arguments.
--- @param x number
--- @param ... number
--- @return integer
--- @nodiscard
function bit.bxor(x, ...) end

--- Returns the logical left shift of `x` by `n` bits.
---
--- Only the lower 5 bits of `n` are used, so the shift count is in `0..31`.
--- @param x number
--- @param n integer
--- @return integer
--- @nodiscard
function bit.lshift(x, n) end

--- Returns the logical right shift of `x` by `n` bits.
---
--- Vacant bits are filled with zeros. Only the lower 5 bits of `n` are used.
--- @param x number
--- @param n integer
--- @return integer
--- @nodiscard
function bit.rshift(x, n) end

--- Returns the arithmetic right shift of `x` by `n` bits.
---
--- The sign bit is replicated. Only the lower 5 bits of `n` are used.
--- @param x number
--- @param n integer
--- @return integer
--- @nodiscard
function bit.arshift(x, n) end

--- Returns `x` rotated left by `n` bits.
---
--- Only the lower 5 bits of `n` are used.
--- @param x number
--- @param n integer
--- @return integer
--- @nodiscard
function bit.rol(x, n) end

--- Returns `x` rotated right by `n` bits.
---
--- Only the lower 5 bits of `n` are used.
--- @param x number
--- @param n integer
--- @return integer
--- @nodiscard
function bit.ror(x, n) end

--- Swaps the bytes of `x` (little-endian to big-endian or vice versa).
--- @param x number
--- @return integer
--- @nodiscard
function bit.bswap(x) end

return bit

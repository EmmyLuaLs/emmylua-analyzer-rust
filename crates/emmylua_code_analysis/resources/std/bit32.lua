--- Copy from Lua Sumneko Lua

--- @meta bit32
--- @version 5.2

--- @version 5.2
---
---
---
--- [View documents](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32"])
---
--- @class bit32lib
bit32 = {}

---
--- Returns the number `x` shifted `disp` bits to the right. Negative displacements shift to the left.
---
--- This shift operation is what is called arithmetic shift. Vacant bits on the left are filled with copies of the higher bit of `x`; vacant bits on the right are filled with zeros.
---
---
--- [View documents](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.arshift"])
---
--- @param x    number
--- @param disp number
--- @return integer
--- @nodiscard
function bit32.arshift(x, disp) end

---
--- Returns the bitwise *and* of its operands.
---
--- [View documents](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.band"])
---
--- @param ... number
--- @return integer
--- @nodiscard
function bit32.band(...) end

---
--- Returns the bitwise negation of `x`.
---
--- ```lua
--- assert(bit32.bnot(x) ==
--- (-1 - x) % 2^32)
--- ```
---
---
--- [View documents](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.bnot"])
---
--- @param x number
--- @return integer
--- @nodiscard
function bit32.bnot(x) end

---
--- Returns the bitwise *or* of its operands.
---
--- [View documents](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.bor"])
---
--- @param ... number
--- @return integer
--- @nodiscard
function bit32.bor(...) end

---
--- Returns a boolean signaling whether the bitwise *and* of its operands is different from zero.
---
--- [View documents](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.btest"])
---
--- @param ... number
--- @return boolean
--- @nodiscard
function bit32.btest(...) end

---
--- Returns the bitwise *exclusive or* of its operands.
---
--- [View documents](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.bxor"])
---
--- @param ... number
--- @return integer
--- @nodiscard
function bit32.bxor(...) end

---
--- Returns the unsigned number formed by the bits `field` to `field + width - 1` from `n`.
---
--- [View documents](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.extract"])
---
--- @param n      number
--- @param field  number
--- @param width? number
--- @return integer
--- @nodiscard
function bit32.extract(n, field, width) end

---
--- Returns a copy of `n` with the bits `field` to `field + width - 1` replaced by the value `v` .
---
--- [View documents](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.replace"])
---
--- @param n      number
--- @param v      number
--- @param field  number
--- @param width? number
--- @return integer
--- @nodiscard
function bit32.replace(n, v, field, width) end

---
--- Returns the number `x` rotated `disp` bits to the left. Negative displacements rotate to the right.
---
--- [View documents](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.lrotate"])
---
--- @param x    number
--- @param disp number
--- @return integer
--- @nodiscard
function bit32.lrotate(x, disp) end

---
--- Returns the number `x` shifted `disp` bits to the left. Negative displacements shift to the right. In any direction, vacant bits are filled with zeros.
---
--- ```lua
--- assert(bit32.lshift(b, disp) ==
--- (b * 2^disp) % 2^32)
--- ```
---
---
--- [View documents](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.lshift"])
---
--- @param x    number
--- @param disp number
--- @return integer
--- @nodiscard
function bit32.lshift(x, disp) end

---
--- Returns the number `x` rotated `disp` bits to the right. Negative displacements rotate to the left.
---
--- [View documents](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.rrotate"])
---
--- @param x    number
--- @param disp number
--- @return integer
--- @nodiscard
function bit32.rrotate(x, disp) end

---
--- Returns the number `x` shifted `disp` bits to the right. Negative displacements shift to the left. In any direction, vacant bits are filled with zeros.
---
--- ```lua
--- assert(bit32.rshift(b, disp) ==
--- math.floor(b % 2^32 / 2^disp))
--- ```
---
---
--- [View documents](command:extension.lua.doc?["en-us/54/manual.html/pdf-bit32.rshift"])
---
--- @param x    number
--- @param disp number
--- @return integer
--- @nodiscard
function bit32.rshift(x, disp) end

return bit32

---@meta
-- Copyright (c) 2018. tangzx(love.tangzx@qq.com)
--
-- Licensed under the Apache License, Version 2.0 (the "License"); you may not
-- use this file except in compliance with the License. You may obtain a copy of
-- the License at
--
-- http://www.apache.org/licenses/LICENSE-2.0
--
-- Unless required by applicable law or agreed to in writing, software
-- distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
-- WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
-- License for the specific language governing permissions and limitations under
-- the License.

--- @class mathlib
math = {}

--- 返回 `x` 的绝对值。（整数/浮点数）
--- @overload fun(x: integer): integer
--- @param x number
--- @return number
function math.abs(x) end

--- 返回 `x` 的反余弦值（以弧度为单位）。
--- @param x number
--- @return number
function math.acos(x) end

--- 返回 `x` 的反正弦值（以弧度为单位）。
--- @param x number
--- @return number
function math.asin(x) end

--- 返回 `y/x` 的反正切值（以弧度为单位），通过两个参数的符号来确定
--- 结果所在的象限。（它也正确处理 `x` 为零的情况。）
---
--- `x` 的默认值为 1，因此调用 `math.atan(y)` 返回 `y` 的反正切值。
--- @param y  number
--- @param x? number
--- @return number
function math.atan(y, x) end

--- 返回大于或等于 `x` 的最小整数。
--- @param x number
--- @return integer
function math.ceil(x) end

--- 返回 `x` 的余弦值（假设为弧度）。
--- @param x number
--- @return number
function math.cos(x) end

--- 将角度 `x` 从弧度转换为角度。
--- @param x number
--- @return number
function math.deg(x) end

--- 返回 *e^x* 的值（其中 e 是自然对数的底）。
--- @param x number
--- @return number
function math.exp(x) end

--- 返回小于或等于 `x` 的最大整数。
--- @param x number
--- @return integer
function math.floor(x) end

--- 返回 `x` 除以 `y` 的余数，该除法将商向零取整。（整数/浮点数）
--- @param x number
--- @param y number
--- @return number
function math.fmod(x, y) end

--- 浮点值 `HUGE_VAL`，一个大于任何其他数值的值。
--- 它是 INF 值，大于 math.maxinteger。
--- @type number
math.huge = nil

--- 返回 `x` 在给定底数下的对数。`base` 的默认值为 *e*
--- （因此函数返回 `x` 的自然对数）。
--- @param x     number
--- @param base? number
--- @return number
function math.log(x, base) end

--- 返回具有最大值的参数，根据 Lua 运算符 `<` 比较。（整数/浮点数）
--- @overload fun(x: integer, ...: integer): integer
--- @param x   number
--- @param ... number
--- @return number
function math.max(x, ...) end

--- @version >5.3
--- 整数类型的最大值。
--- @type integer
math.maxinteger = nil

--- 返回具有最小值的参数，根据 Lua 运算符 `<` 比较。（整数/浮点数）
--- @overload fun(x: integer, ...: integer): integer
--- @param x   number
--- @param ... number
--- @return number
function math.min(x, ...) end

--- @version >5.3
--- 整数类型的最小值。
--- @type integer
math.mininteger = nil

--- 返回 `x` 的整数部分和小数部分。其第二个结果始终为浮点数。
--- @param x number
--- @return integer
--- @return number
function math.modf(x) end

--- π 的值。
math.pi = 3.1415

--- 将角度 `x` 从角度转换为弧度。
--- @param x number
--- @return number
function math.rad(x) end

--- 当不带参数调用时，返回一个在范围 *[0,1)* 内均匀分布的伪随机浮点数。
--- 当带两个整数 `m` 和 `n` 调用时，`math.random` 返回一个在范围 *[m, n]*
--- 内均匀分布的伪随机整数。调用 `math.random(n)` 相当于 `math.random(1,n)`。
--- @overload fun(): number
--- @overload fun(m: integer): integer
--- @param m integer
--- @param n integer
--- @return integer
function math.random(m, n) end

--- Sets `x` as the "seed" for the pseudo-random generator: equal seeds
--- produce equal sequences of numbers.
--- @param x integer
function math.randomseed(x) end

--- @version > 5.4
--- 当至少带有一个参数调用时，整数参数 `x` 和 `y` 被组合成一个 128 位种子，
--- 用于重新初始化伪随机生成器；相同的种子会产生相同的数字序列。`y` 的默认值为零。
---
--- 当不带参数调用时，Lua 会生成一个种子，但随机性较弱。
---
--- 此函数会返回实际使用的两个种子组成部分，以便再次设置它们时能够重复该序列。
--- @param x? integer
--- @param y? integer
--- @return integer, integer
function math.randomseed(x, y) end

--- 返回 `x` 的正弦值（假设为弧度）。
--- @param x number
--- @return number
function math.sin(x)
    return 0
end

--- 返回 `x` 的平方根。（你也可以使用表达式 `x^0.5` 来计算此值。）
--- @param x number
--- @return number
function math.sqrt(x)
    return 0
end

--- 返回 `x` 的正切值（假设为弧度）。
--- @param x number
--- @return number
function math.tan(x)
    return 0
end

--- @version >5.3
--- 如果值 `x` 可转换为整数，则返回该整数。
--- 否则，返回 `nil`。
--- @param x any
--- @return integer?
function math.tointeger(x) end

--- @version >5.3
--- 如果 `x` 是整数，返回 "`integer`"；如果它是浮点数，返回 "`float`"；
--- 如果 `x` 不是数字，返回 **nil**。
--- @param x any
--- @return 'integer'|'float'|nil
function math.type(x) end

--- @version >5.3
--- 返回一个布尔值，当且仅当整数 `m` 在作为无符号整数与 `n` 比较时
--- 小于 `n` 时为 true。
--- @param m number
--- @param n number
--- @return boolean
function math.ult(m, n) end

--- @version 5.1, 5.2, JIT
--- 返回 `x` 的 `y` 次幂。(x^y)
--- @param x number 底数
--- @param y number 指数
--- @return number
function math.pow(x, y) end

--- @version 5.1, 5.2, JIT
--- 返回 `y/x` 的反正切值（以弧度为单位），通过两个参数的符号来确定
--- 结果所在的象限。（它也正确处理 `x` 为零的情况。）
---
--- 注意：在某些 Lua 实现中，此函数相当于 `math.atan(y, x)`。
--- @param y number
--- @param x number
--- @return number
function math.atan2(y, x) end

--- @version 5.1, JIT
--- 返回 `x` 的以 10 为底的对数。
--- @param x number
--- @return number
function math.log10(x) end

--- @version 5.1, 5.2, JIT
--- 返回 `x` 的双曲余弦值。
--- @param x number
--- @return number
function math.cosh(x) end

--- @version 5.1, 5.2, JIT
--- 返回 `x` 的双曲正弦值。
--- @param x number
--- @return number
function math.sinh(x) end

--- @version 5.1, 5.2, JIT
--- 返回 `x` 的双曲正切值。
--- @param x number
--- @return number
function math.tanh(x) end

--- @version 5.1, 5.2, JIT
--- 返回 `m` 和 `e`，使得 *x = m2^e*，`e` 是整数，且 `m` 的绝对值
--- 在范围 *[0.5, 1)* 内（或者当 `x` 为零时为零）。
--- @param x number
--- @return number, integer
function math.frexp(x) end

--- @version 5.1, 5.2, JIT
--- 返回 *m2e* (`e` 应该是整数)。
--- @param m number
--- @param e integer
--- @return number
function math.ldexp(m, e) end

return math

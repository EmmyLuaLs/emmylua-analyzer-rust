--- @meta
--- @version >5.3

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

--- @version >5.3
--- @class utf8lib
utf8 = {}

--- 接收零个或多个整数，将每个整数转换为对应的 UTF-8 字节序列，并返回将这些序列连接在一起的字符串。
--- @return string
function utf8.char(...) end

--- 模式（一个字符串，而非函数）`[\0-\x7F\xC2-\xF4][\x80-\xBF]*`，
--- 在假定被匹配对象是合法 UTF-8 字符串的情况下，它精确匹配一个 UTF-8 字节序列。
--- @type string
utf8.charpattern = ""

---
--- Returns values so that the construction
--- > `for p, c in utf8.codes(s) do` *body* `end`
--- will iterate over all characters in string `s`, with `p` being the position
--- (in bytes) and `c` the code point of each character. It raises an error if
--- it meets any invalid byte sequence.
--- @param s string
--- @return fun(s: string, i?: integer): integer, integer
function utf8.codes(s) end

--- @version >5.4
--- @param s    string
--- @param lax? boolean
--- @return fun(s: string, i?: integer): integer, integer
function utf8.codes(s, lax) end

---
--- Returns the codepoints (as integers) from all characters in `s` that start
--- between byte position `i` and `j` (both included). The default for `i` is
--- 1 and for `j` is `i`. It raises an error if it meets any invalid byte
--- sequence.
--- @overload fun(s: string): integer
--- @param s  string
--- @param i? integer
--- @param j? integer
--- @return integer
function utf8.codepoint(s, i, j) end

--- @version >5.4
--- @overload fun(s: string): integer
--- @param s    string
--- @param i?   integer
--- @param j?   integer
--- @param lax? boolean
--- @return integer
function utf8.codepoint(s, i, j, lax) end

---
--- Returns the number of UTF-8 characters in string `s` that start between
--- positions `i` and `j` (both inclusive). The default for `i` is 1 and for
--- `j` is -1. If it finds any invalid byte sequence, returns a false value
--- plus the position of the first invalid byte.
--- @param s  string
--- @param i? integer
--- @param j? integer
--- @return_overload integer
--- @return_overload nil, integer errpos
--- @nodiscard
function utf8.len(s, i, j) end

--- @version >5.4
--- @param s    string
--- @param i?   integer
--- @param j?   integer
--- @param lax? boolean
--- @return_overload integer
--- @return_overload nil, integer errpos
--- @nodiscard
function utf8.len(s, i, j, lax) end

--- 返回 `s` 中第 `n` 个字符的编码起始位置（按字节计）（从位置 `i` 开始计数）。
--- 负的 `n` 表示取 `i` 之前的字符。
---
--- `n` 非负时，`i` 的默认值为 1；否则 `i` 的默认值为 `#s + 1`，因此
--- `utf8.offset(s, -n)` 可获得从字符串末尾数第 `n` 个字符的偏移。
---
--- 如果指定字符既不在字符串中也不在其末尾紧接处，则函数返回 nil。
--- 特殊情况：当 `n` 为 0 时，函数返回包含 `s` 的第 `i` 个字节的那个字符的编码起始位置。
---
--- 此函数假定 `s` 是合法的 UTF-8 字符串。
--- @overload fun(s: string): integer
--- @param s  string
--- @param n  integer
--- @param i? integer
--- @return integer
function utf8.offset(s, n, i) end

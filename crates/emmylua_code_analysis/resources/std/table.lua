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

---@class tablelib
table = {}

---
--- Given a list where all elements are strings or numbers, returns the string
--- `list[i]..sep..list[i+1] ... sep..list[j]`. The default value for
--- `sep` is the empty string, the default for `i` is 1, and the default for
--- `j` is #list. If `i` is greater than `j`, returns the empty string.
---@overload fun(list:string[]):string
---@overload fun(list:string[], sep:string):string
---@overload fun(list:string[], sep:string, i:number):string
---@param list string[]
---@param sep string
---@param i number
---@param j number
---@return string
function table.concat(list, sep, i, j) end

---
--- Inserts element `value` at position `pos` in `list`, shifting up the
--- elements to `list[pos]`, `list[pos+1]`, `···`, `list[#list]`. The default
--- value for `pos` is ``#list+1`, so that a call `table.insert(t,x)`` inserts
--- `x` at the end of list `t`.
---@overload fun(list:table, value:any):number
---@param list table
---@param pos number
---@param value any
---@return number
function table.insert(list, pos, value) end

---
--- Moves elements from table a1 to table `a2`, performing the equivalent to
--- the following multiple assignment: `a2[t]`,`··· = a1[f]`,`···,a1[e]`. The
--- default for `a2` is `a1`. The destination range can overlap with the source
--- range. The number of elements to be moved must fit in a Lua integer.
---
--- Returns the destination table `a2`.
---@overload fun(a1:table, f:number, e:number, t:number):table
---@param a1 table
---@param f number
---@param e number
---@param t number
---@param a2 table
---@return table
function table.move(a1, f, e, t, a2) end

---@version 5.1
---
---Returns the largest positive numerical index of the given table, or zero if the table has no positive numerical indices.
---
---@param table table
---@return integer
---@nodiscard
function table.maxn(table) end

---
--- Removes from `list` the element at position `pos`, returning the value of
--- the removed element. When `pos` is an integer between 1 and `#list`, it
--- shifts down the elements `list[pos+1]`, `list[pos+2]`, `···`,
--- `list[#list]` and erases element `list[#list]`; The index pos can also be 0
--- when `#list` is 0, or `#list` + 1; in those cases, the function erases
--- the element `list[pos]`.
---
--- The default value for `pos` is `#list`, so that a call `table.remove(l)`
--- removes the last element of list `l`.
---@generic V
---@param list table<number, V> | V[]
---@param pos? integer
---@return V
function table.remove(list, pos) end

---
--- Sorts list elements in a given order, *in-place*, from `list[1]` to
--- `list[#list]`. If `comp` is given, then it must be a function that receives
--- two list elements and returns true when the first element must come before
--- the second in the final order (so that, after the sort, `i < j` implies not
--- `comp(list[j],list[i]))`. If `comp` is not given, then the standard Lua
--- operator `<` is used instead.
---
--- Note that the `comp` function must define a strict partial order over the
--- elements in the list; that is, it must be asymmetric and transitive.
--- Otherwise, no valid sort may be possible.
---
--- The sort algorithm is not stable: elements considered equal by the given
--- order may have their relative positions changed by the sort.
---@overload fun(list:table):number
---@generic V
---@param list V[]
---@param comp fun(a:V, b:V):boolean
---@return number
function table.sort(list, comp) end

---@version > 5.2, JIT
--- Returns the elements from the given list. This function is equivalent to
--- return `list[i]`, `list[i+1]`, `···`, `list[j]`
--- By default, i is 1 and j is #list.
---@generic T
---@param i? number
---@param j? number
---@param list [T...]
---@return T...
function table.unpack(list, i, j) end

---@version > 5.2, JIT
---
---Returns a new table with all arguments stored into keys `1`, `2`, etc. and with a field `"n"` with the total number of arguments.
---
---@generic T
---@param ... T...
---@return [T...] | { n: integer }
---@nodiscard
function table.pack(...) end

---@version 5.1, JIT
---
---Executes the given f over all elements of table. For each element, f is called with the index and respective value as arguments. If f returns a non-nil value, then the loop is broken, and this value is returned as the final value of foreach.
---
---
---@generic T
---@param list any
---@param callback fun(key: string, value: any):T|nil
---@return T?
---@deprecated
function table.foreach(list, callback) end


---
---Executes the given f over the numerical indices of table. For each index, f is called with the index and respective value as arguments. Indices are visited in sequential order, from 1 to n, where n is the size of the table. If f returns a non-nil value, then the loop is broken and this value is returned as the result of foreachi.
---
---@version 5.1, JIT
---@generic T
---@param list any
---@param callback fun(key: string, value: any):T|nil
---@return T?
---@deprecated
function table.foreachi(list, callback) end


---
---Returns the number of elements in the table. This function is equivalent to `#list`.
---
---[View documents](command:extension.lua.doc?["en-us/54/manual.html/pdf-table.getn"])
---@version 5.1, JIT
---@generic T
---@param list T[]
---@return integer
---@nodiscard
---@deprecated
function table.getn(list) end

return table

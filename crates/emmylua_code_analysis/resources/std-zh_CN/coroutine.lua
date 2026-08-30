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

--- @class coroutinelib
coroutine = {}

--- 创建一个新的协程，主体函数为 `f`。`f` 必须是一个 Lua 函数。
--- 返回这个新协程，一个类型为 `"thread"` 的对象。
--- @param f async fun(...): any...
--- @return thread
--- @nodiscard
function coroutine.create(f) end

--- 当正在运行的协程可以让出时返回 true。
---
--- 如果正在运行的协程不是主线程且不在不可让出的 C 函数中，
--- 则该协程是可让出的。
--- @param co? thread
--- @return boolean
--- @nodiscard
function coroutine.isyieldable(co) end

--- @version >5.4
--- 关闭协程 `co`，关闭其所有待关闭的变量，并将协程置于死亡状态。
--- @param co thread
--- @return boolean noerror
--- @return any errorobject
function coroutine.close(co) end

--- 开始或继续执行协程 `co`。第一次恢复协程时，它开始运行其主体函数。
--- 值 `val1`, ... 作为参数传递给主体函数。如果协程已让出，`resume`
--- 会重新启动它；值 `val1`, ... 作为 yield 的返回结果传递。
---
--- 如果协程运行时没有任何错误，`resume` 返回 **true** 加上传递给
--- `yield` 的任何值（当协程让出时）或主体函数返回的任何值（当协程
--- 终止时）。如果有任何错误，`resume` 返回 **false** 加上错误消息。
--- @param co    thread
--- @param val1? any
--- @param ...   any
--- @return boolean success
--- @return any ...
function coroutine.resume(co, val1, ...) end

--- @version 5.1, JIT
---
--- Returns the running coroutine, or nil when called by the main thread.
--- @return thread?
--- @nodiscard
function coroutine.running() end

--- @version >5.2
--- 返回正在运行的协程加上一个布尔值，当正在运行的协程是主协程时为 true。
--- @return thread, boolean
--- @nodiscard
function coroutine.running() end

--- 以字符串形式返回协程 `co` 的状态："`running`"，如果协程正在运行
--- （即它调用了 `status`）；"`suspended`"，如果协程在调用 `yield`
--- 时挂起，或者尚未开始运行；"`normal`"，如果协程处于活动状态但未
--- 运行（即它已恢复另一个协程）；"`dead`"，如果协程已完成其主体函数，
--- 或者因错误而停止。
--- @param co thread
--- @return
--- | "running" # 正在运行
--- | "suspended" # 已暂停或未开始
--- | "normal" # 活动但未运行
--- | "dead" # 完成或错误停止
--- @nodiscard
function coroutine.status(co) end

--- 创建一个新的协程，主体函数为 `f`。`f` 必须是一个 Lua 函数。
--- 返回一个函数，每次调用该函数时都会恢复协程。传递给该函数的任何
--- 参数都作为 `resume` 的额外参数。返回与 `resume` 相同的值，
--- 但不包括第一个布尔值。如果发生错误，则传播该错误。
--- @param f async fun(...): any...
--- @return fun(...): any...
--- @nodiscard
function coroutine.wrap(f) end

--- 挂起调用协程的执行。传递给 `yield` 的任何参数都作为额外结果
--- 传递给 `resume`。
--- @async
--- @param ... any
--- @return any ...
function coroutine.yield(...) end

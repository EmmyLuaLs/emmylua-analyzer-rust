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

--- @class oslib
os = {}

--- 返回程序使用的 CPU 时间（以秒为单位）的近似值。
--- @return number
function os.clock() end

--- @class std.osdateparam
--- @field year  integer | string    四位年份
--- @field month integer | string    月 (1-12)
--- @field day   integer | string    日 (1-31)
--- @field hour  (integer | string)? 时 (0-23)
--- @field min   (integer | string)? 分 (0-59)
--- @field sec   (integer | string)? 秒 (0-61，包含闰秒)
--- @field wday  (integer | string)? 星期几 (1-7，星期日为 1)
--- @field yday  (integer | string)? 一年中的哪一天 (1-366)
--- @field isdst boolean?            夏令时标志，布尔值。

--- @class std.osdate: std.osdateparam
--- @field year  integer | string 四位年份
--- @field month integer | string 月 (1-12)
--- @field day   integer | string 日 (1-31)
--- @field hour  integer | string 时 (0-23)
--- @field min   integer | string 分 (0-59)
--- @field sec   integer | string 秒 (0-61，包含闰秒)
--- @field wday  integer | string 星期几 (1-7，星期日为 1)
--- @field yday  integer | string 一年中的哪一天 (1-366)
--- @field isdst boolean          夏令时标志，布尔值。

--- 返回包含日期和时间的字符串或表，格式由字符串 `format` 指定。
---
--- 如果提供了 `time` 参数，则格式化该时间（有关此值的描述，请参见
--- `os.time` 函数）。否则，`date` 格式化当前时间。
---
--- 如果 `format` 以 '`!`' 开头，则日期按协调世界时（UTC）格式化。
--- 在此可选字符之后，如果 `format` 是字符串 "`*t`"，则 `date`
--- 返回一个包含以下字段的表：
---
--- **`year`** (四位年份)
--- **`month`** (1-12)
--- **`day`** (1-31)
--- **`hour`** (0-23)
--- **`min`** (0-59)
--- **`sec`** (0-61, 包含闰秒)
--- **`wday`** (星期几, 1-7, 星期日为 1)
--- **`yday`** (一年中的哪一天, 1-366)
--- **`isdst`** (夏令时标志, 布尔值). 如果信息不可用，可能会缺少此字段。
---
--- 如果 `format` 不是 "`*t`"，则 `date` 将日期作为字符串返回，
--- 其格式化规则与 ISO C 函数 `strftime` 相同。
---
--- 当不带参数调用时，`date` 返回依赖于主机系统和当前区域设置的
--- 合理日期和时间表示。（更具体地说，`os.date()` 相当于 `os.date("%c")`。）
---
--- 在非 POSIX 系统上，此函数可能不是线程安全的，因为它依赖于
--- C 函数 `gmtime` 和 `localtime`。
--- @overload fun(fmt: "*t", time?: number): std.osdate
--- @overload fun(fmt: "!*t", time?: number): std.osdate
--- @param format? string
--- @param time?  number
--- @return string
function os.date(format, time) end

--- 返回从时间 `t1` 到时间 `t2` 的差值（以秒为单位）。（其中时间是
--- `os.time` 返回的值）。在 POSIX、Windows 和其他一些系统中，
--- 此值正好是 `t2`-`t1`。
--- @param t2 number
--- @param t1 number
--- @return number
function os.difftime(t2, t1) end

--- @version >5.2
--- 此函数相当于 C 函数 `system`。它传递 `command` 给操作系统 shell 执行。
--- 如果命令成功终止，它的第一个结果是 **true**，否则是 **nil**。
---
--- 在第一个结果之后，函数返回一个字符串加上一个数字：
--- - **"exit"**: 命令正常终止；后面的数字是命令的退出状态。
--- - **"signal"**: 命令被信号终止；后面的数字是终止命令的信号。
---
--- 当不带命令调用时，如果 shell 可用，`os.execute` 返回 true。
--- @overload fun(): boolean
--- @param command string
--- @return true|nil
--- @return 'exit'|'signal'
--- @return integer
function os.execute(command) end

--- @version 5.1, JIT
---
--- This function is equivalent to the C function system. It passes command to
--- be executed by an operating system shell. It returns a status code, which is
--- system-dependent. If command is absent, then it returns nonzero if a shell
--- is available and zero otherwise.
--- @param command string
--- @return integer
function os.execute(command) end

--- @version >5.2, JIT
--- 调用 ISO C 函数 `exit` 以终止宿主程序。
---
--- 如果 `code` 为 **true**，返回的状态是 `EXIT_SUCCESS`；
---
--- 如果 `code` 为 **false**，返回的状态是 `EXIT_FAILURE`；
---
--- 如果 `code` 是数字，返回的状态就是该数字。
---
--- `code` 的默认值为 **true**。
---
--- 如果可选的第二个参数 `close` 为 true，则在退出前关闭 Lua 状态。
--- @param code?  boolean | integer
--- @param close? boolean
function os.exit(code, close) end

--- @version 5.1
---
--- Calls the C function exit, with an optional `code`, to terminate the host
--- program. The default value for `code` is the success code.
--- @param code? integer
function os.exit(code) end

--- 返回进程环境变量 `varname` 的值，如果未定义该变量，则返回 **nil**。
--- @param varname string
--- @return string?
function os.getenv(varname) end

--- 删除具有给定名称的文件（或在 POSIX 系统上的空目录）。
--- 如果此函数失败，它返回 **nil**，加上描述错误的字符串和错误代码。
--- 否则，它返回 true。
--- @param filename string
--- @return true|nil result
--- @return string err
function os.remove(filename) end

--- 将名为 `oldname` 的文件或目录重命名为 `newname`。如果此函数失败，
--- 它返回 **nil**，加上描述错误的字符串和错误代码。否则，它返回 true。
--- @param oldname string
--- @param newname string
--- @return true|nil result
--- @return string err
function os.rename(oldname, newname) end

--- 设置程序的当前区域设置。`locale` 是一个依赖于系统的字符串，指定区域设置；
--- `category` 是一个可选字符串，描述要更改的类别：`"all"`, `"collate"`,
--- `"ctype"`, `"monetary"`, `"numeric"`, 或 `"time"`；默认类别是 `"all"`。
--- 该函数返回新区域设置的名称，如果无法满足请求，则返回 **nil**。
---
--- 如果 `locale` 是空字符串，当前区域设置将设置为实现定义的本机区域设置。
--- 如果 `locale` 是字符串 "`C`"，当前区域设置将设置为标准 C 区域设置。
---
--- 当使用 **nil** 作为第一个参数调用时，此函数仅返回给定类别的当前区域设置名称。
---
--- 此函数可能不是线程安全的，因为它依赖于 C 函数 `setlocale`。
--- @param locale    string
--- @param category? string
--- @return string|nil
function os.setlocale(locale, category) end

--- 当不带参数调用时返回当前时间，或者返回由给定表指定的日期和时间对应的时间。
--- 此表必须有 `year`、`month` 和 `day` 字段，并且可以有 `hour`（默认为 12）、
--- `min`（默认为 0）、`sec`（默认为 0）和 `isdst`（默认为 **nil**）。
--- 忽略其他字段。有关这些字段的描述，请参见 `os.date` 函数。
---
--- 调用该函数时，这些字段中的值不需要在其有效范围内。例如，如果 `sec` 为 -10，
--- 则表示在其他字段指定的时间之前 10 秒；如果 `hour` 为 1000，则表示在
--- 其他字段指定的时间之后 1000 小时。
---
--- 返回的值是一个数字，其含义取决于你的系统。在 POSIX、Windows 和其他一些系统中，
--- 此数字计算自某个给定开始时间（"epoch"）以来的秒数。在其他系统中，
--- 含义未指定，`time` 返回的数字只能用作 `os.date` 和 `os.difftime` 的参数。
---
--- 当使用表调用时，`os.time` 还会规范化 `os.date` 函数中记录的所有字段，
--- 以便它们表示与调用前相同的时间，但值在其有效范围内。
--- @param date? std.osdateparam
--- @return integer
function os.time(date) end

--- 返回一个包含可用于临时文件文件名的字符串。该文件必须在使用前显式打开，
--- 不再需要时显式删除。
---
--- 在某些系统（POSIX）上，此函数还会创建一个具有该名称的文件，以避免安全风险。
--- （其他人可能会在获取名称和创建文件之间的时间内创建具有错误权限的文件。）
--- 你仍然必须打开文件才能使用它并删除它（即使你不使用它）。
---
--- 如果可能，你可能更喜欢使用 `io.tmpfile`，它在程序结束时自动删除文件。
--- @return string
function os.tmpname() end

return os

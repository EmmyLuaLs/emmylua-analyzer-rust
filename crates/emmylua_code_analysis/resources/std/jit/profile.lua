--- Copy from Lua Sumneko Lua
--- @meta jit.profile
--- @version JIT

local profile = {}

--- @param mode? string
--- @param func  fun(L: thread, samples: integer, vmstate: string)
function profile.start(mode, func) end

function profile.stop() end

--- @overload fun(th: thread, fmt: string, depth: integer): string
--- @param fmt   string
--- @param depth integer
--- @return string dump
function profile.dumpstack(fmt, depth) end

return profile

--- Copy from Lua Sumneko Lua
--- @meta jit
--- @version JIT

--- @version JIT
--- @class jitlib
--- @field version     string
--- @field version_num integer
--- @field os          'Windows' | 'Linux' | 'OSX' | 'BSD' | 'POSIX' | 'Other'
--- @field arch        'x86' | 'x64' | 'arm' | 'arm64' | 'arm64be' | 'ppc' | 'ppc64' | 'ppc64le' | 'mips' | 'mipsel' | 'mips32r6' | 'mips32r6el' | 'mips64' | 'mips64el' | 'mips64r6' | 'mips64r6el' | string
jit = {}

--- @param func?      function | boolean
--- @param recursive? boolean
function jit.on(func, recursive) end

--- @param func?      function | boolean
--- @param recursive? boolean
function jit.off(func, recursive) end

--- @param func?      function | boolean | integer
--- @param recursive? boolean
function jit.flush(func, recursive) end

--- @return boolean status
--- @return string ...
--- @nodiscard
function jit.status() end

jit.opt = {}

--- @param ... string | number
function jit.opt.start(...) end

--- @param mode? 'prng' | 'strhash' | 'strid' | 'mcode'
--- @return integer status
--- @nodiscard
function jit.security(mode) end

--- @param func   function
--- @param event? string
function jit.attach(func, event) end

return jit

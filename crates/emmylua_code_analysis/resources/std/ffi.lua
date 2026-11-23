--- Copy from Lua Sumneko Lua

--- @meta ffi
--- @version JIT

--- @class ffi.namespace*: userdata
--- @field [string] ffi.cdata* | number | boolean

--- @class ffi.clib*: ffi.namespace*

--- @class ffi.ctype*: userdata
--- @overload fun(init?: any, ...): ffi.cdata*
--- @overload fun(nelem?: integer, init?: any, ...): ffi.cdata*
local ctype

--- @class ffi.cdecl*: string
--- @class ffi.cdata*: userdata
--- @alias ffi.ct* ffi.ctype* | ffi.cdecl* | ffi.cdata*
--- @class ffi.cb*: ffi.cdata*
local cb
--- @class ffi.VLA*: userdata
--- @class ffi.VLS*: userdata

--- @version JIT
--- @class ffilib
--- @field C    ffi.clib*
--- @field os   'Windows' | 'Linux' | 'OSX' | 'BSD' | 'POSIX' | 'Other'
--- @field arch
--- | 'x86'
--- | 'x64'
--- | 'arm'
--- | 'arm64'
--- | 'arm64be'
--- | 'ppc'
--- | 'ppc64'
--- | 'ppc64le'
--- | 'mips'
--- | 'mipsel'
--- | 'mips32r6'
--- | 'mips32r6el'
--- | 'mips64'
--- | 'mips64el'
--- | 'mips64r6'
--- | 'mips64r6el'
--- | string
local ffi = {}

--- @param def     string
--- @param params? any
function ffi.cdef(def, params, ...) end

--- @param name    string
--- @param global? boolean
--- @return ffi.clib* clib
--- @nodiscard
function ffi.load(name, global) end

--- @overload fun(ct: ffi.ct*, init: any, ...): ffi.cdata*
--- @param ct     ffi.ct*
--- @param nelem? integer
--- @param init?  any
--- @return ffi.cdata* cdata
--- @nodiscard
function ffi.new(ct, nelem, init, ...) end

--- @param ct      ffi.ct*
--- @param params? any
--- @return ffi.ctype* ctype
--- @nodiscard
function ffi.typeof(ct, params, ...) end

--- @param ct   ffi.ct*
--- @param init any
--- @return ffi.cdata* cdata
--- @nodiscard
function ffi.cast(ct, init) end

--- @param ct        ffi.ct*
--- @param metatable table
--- @return ffi.ctype* ctype
function ffi.metatype(ct, metatable) end

--- @param cdata     ffi.cdata*
--- @param finalizer function | ffi.cdata* | nil
--- @return ffi.cdata* cdata
function ffi.gc(cdata, finalizer) end

--- @param ct     ffi.ct*
--- @param nelem? integer
--- @return integer|nil size
--- @nodiscard
function ffi.sizeof(ct, nelem) end

--- @param ct ffi.ct*
--- @return integer align
--- @nodiscard
function ffi.alignof(ct) end

--- @param ct    ffi.ct*
--- @param field string
--- @return integer ofs
--- @return integer? bpos
--- @return integer? bsize
--- @nodiscard
function ffi.offsetof(ct, field) end

--- @param ct  ffi.ct*
--- @param obj any
--- @return boolean status
--- @nodiscard
function ffi.istype(ct, obj) end

--- @param newerr? integer
--- @return integer err
--- @nodiscard
function ffi.errno(newerr) end

--- @param ptr  ffi.cdata* | string
--- @param len? integer
--- @return string str
function ffi.string(ptr, len) end

--- @overload fun(dst: ffi.cdata*, str: string)
--- @param dst ffi.cdata*
--- @param src ffi.cdata* | string
--- @param len integer
function ffi.copy(dst, src, len) end

--- @param dst ffi.cdata*
--- @param len integer
--- @param c?  integer
function ffi.fill(dst, len, c) end

--- @param param
--- | '32bit'
--- | '64bit'
--- | 'le'
--- | 'be'
--- | 'fpu'
--- | 'softfp'
--- | 'hardfp'
--- | 'eabi'
--- | 'win'
--- | 'pauth'
--- | 'uwp'
--- | 'gc64'
--- | 'dualnum'
--- | string
--- @return boolean status
function ffi.abi(param) end

function cb:free() end

--- @param func function
function cb:set(func) end

return ffi

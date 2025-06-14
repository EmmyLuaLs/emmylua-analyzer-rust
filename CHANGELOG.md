# CHANGELOG

# 0.8.2 (unreleased)

`CHG` When a function call jumps, if there are multiple declarations, It will then attempt to return the most matching definition along with all actual code declarations, rather than returning all definitions.

# 0.8.1

`CHG` Generic constraint (StrTplRef) removes the protection for string: 
```lua
---@generic T: string -- need to remove `: string`
---@param a `T`
---@return T
local function class(a)
end

---@class A

local A = class("A") -- error
```

`NEW` Explicitly declared `Tuple` are immutable.
```lua
---@type [1, 2]
local a = {1, 2}

a[1] = 3 -- error
```

`FIX` Hover `function` now can show the corresponding doc comment.

`FIX` Fix a crash when `go to definition` of member

`NEW` Added the configuration item `classDefaultCall`, which is used to declare a method with the specified name as the default `__call` for a class. The effect is equivalent to `---@overload fun()`, but with a lower priority. If an explicitly declared `---@overload fun()` exists, `classDefaultCall` will have no effect on the class.

```json
{
  "runtime": {
    "classDefaultCall": {
      "functionName": "__init",
      "forceNonColon": true,
      "forceReturnSelf": true
    }
  },
}
```

```lua
---@class MyClass
local M = {}

-- `functionName` is `__init`, so the call will be treated as `__init`
function M:__init(a)
    -- `forceReturnSelf` is `true`, so the call will return `self`. even if the method does not return `self` or returns some other value.
end


-- `forceNonColon` is `true`, so the call can be called without `:` and without passing `self`
-- `forceReturnSelf` is `true`, so the call will return `self`
A = M() -- `A` is `MyClass`
```


`NEW` Added `docBaseConstMatchBaseType` configuration item, default is `false`. Base constant types defined in doc can match base types, allowing int to match `---@alias id 1|2|3`, same for string.

```json
{
  "strict": {
    "docBaseConstMatchBaseType": true
  },
}
```

`FIX` When `enum` is used as a function parameter, it is treated as a value rather than the `enum` itself.

`FIX` When the expected type of a table field is a function, function completion is available.

`NEW` `inlay_hint` params hint can now jump to the actual type definition.

`NEW` When closing files in the editor that are not in the workspace or library, their impact will be removed

`NEW` Enhanced Ignore-related functionality. Files configured to be ignored will not be parsed even when opened, but due to technical constraints, files already open when starting the editor will still be parsed (to be fixed in a future update)

# 0.8.0

`NEW` Implement `std.Unpack` type for better type inference of the `unpack` function, and `std.Rawget` type for better type inference of the `rawget` function

`NEW` Support `generator` implementation similar to `luals`

`FIX` Fix issue where type narrowing is lost in nested closures

`NEW` Improved generic parameter inference for lambda functions, now better inferring parameter types for lambda functions

`CHG` Changed `math.huge` to number type

`FIX` Optimized inference of variadic generic return values, now usable for asynchronous library return value inference:
```lua
--- @generic T, R
--- @param argc integer
--- @param func fun(...:T..., cb: fun(...:R...))
--- @return async fun(...:T...):R...
local function wrap(argc, func) end

--- @param a string
--- @param b string
--- @param callback fun(out: string)
local function system(a, b, callback) end

local wrapped = wrap(3, system)
```

`FIX` Optimized rendering of certain type hints

`NEW` Added documentation hints for modules and types in code completion

`NEW` Added type checking for intersection types

`NEW` Support for generic constraint checking, string template parameter type checking and code completion

`FIX` Fix performance issue where type checking drastically slows down when large Lua tables are present in the project, causing the entire project to become unresponsive

# 0.7.3

`FIX` Fix a crash issue

`NEW` Support `@return_cast` for functions. When a function's return value is boolean (must be annotated as boolean), you can add an additional annotation `---@return_cast <param> <cast op>`, indicating that when the function returns true, the parameter `<param>` will be transformed to the corresponding type according to the cast. For example:
```lua
---@return boolean
---@return_cast n integer
local function isInteger(n)
    return n == math.floor(n)
end

local a ---@type integer | string

if isInteger(a) then
    print(a) -- a: integer
else
    print(a) -- a: string
end
```

`@return_cast` support self param. For example:
```lua
---@class My2

---@class My1

---@class My3:My2,My1
local m = {}


---@return boolean
---@return_cast self My1
function m:isMy1()
end

---@return boolean
---@return_cast self My2
function m:isMy2()
end

if m:isMy1() then
    print(m) -- m: My1
elseif m:isMy2() then
    print(m) -- m: My2
end
```

`CHG` Remove diagnostic `lua-syntax-error`, it merges into `syntax-error`, add `doc-syntax-error` for doc syntax error

`FIX` Fix format issue, Now When exist `syntax-error`, the format never return value

`FIX` Fix a performance issue: prevent large union types when functions return tables

`CHG` When an object returned by require function is a class/enum, defining new members on it is prohibited, while tables are not restricted

`NEW` Support `Lua 5.5` global decl grammar

`NEW` Support `TypeGuard<T>` as return type. For example:
```lua

---@return TypeGuard<string>
local function is_string(value)
    return type(value) == "string"
end

local a

if is_string(a) then
    print(a:sub(1, 1))
else
    print("a is not a string")
end
```


# 0.7.2

`FIX` Fix reading configuration file encoded with UTF-8 BOM

`NEW` Support `Call hierarchy` but only support incomming call

`NEW` Support new tag `@internal` for members or declarations. When a member or declaration is marked as `@internal`, it is only visible within its current library. This means that if you use `@internal` in one library, you cannot access this member or declaration from other libraries or workspace.

`NEW` Support `Go to implementation`

`NEW` Support `@nodiscard` with reason

`FIX` Fix Some performance issue

# 0.7.1

`NEW` Now language server configuration might be provided globally via the `<os-specific home dir>/.emmyrc.json`, `<os-specific config dir>/emmylua_ls/.emmyrc.json`, or by setting a variable `EMMYLUALS_CONFIG` with a path to the json configuration.
Global configuration have less priority than the local one

`NEW` Classes might now infer from generic types and provide corresponding completions.

`CHG` Refactor flow analyze algorithm

`NEW` Array return values are now considered nullable. If you want to remove this behavior, you can set `strict.arrayIndex` to `false` in the configuration file.

`FIX` Fix some self infer issue 

`FIX` Fix some diagnostic action issue

`FIX` Optimize some type check

`FIX` Optimize some completion

# 0.7.0 

`CHG` Refactor `type infer`

`CHG` Refactor `member infer`

`FIX` Optimize and Fix tuple type check

`NEW` Support Varidic type use in tuple, eg: `[string, integer...]`

`FIX` Optimize pcall infer, now can match the self and alias

`FIX` for range iter var now will remove nil type

`FIX` Optimize some std library type check

`NEW` Support infer from setmetatable

`NEW` emmylua_doc_cli will export more information

`NEW` Optimize type check rule for subclass and super class

`NEW` Allow '-' as description

# 0.6.0

`NEW` Disable re-index in default, need to enable by `workspace.enableReindex`

`NEW` Add New Diagnostics `inject_field`, `missing_fields`, `redefined_local`, `undefined_field`, `inject-field`, `missing-global-doc`, 
`incomplete-signature-doc`, `circle-doc-class`, `assign-type-mismatch`, `unbalanced_assignments`, `check_return_count`, `duplicate_require`, `circle_doc_class`, `incomplete_signature_doc`, `unnecessary_assert`

`NEW` Support `true` and `false` as type

`NEW` Compact luals fun return syntax like: `(name: string, age: number)`

`NEW` Aliases and overloads of iterator functions (i.e `fun(v: any): (K, V)` where `K` is the key type and `V` is the value type) are now used to infer types in `for` loops

`NEW` Compact luals string template syntax like: xxx`T`, `T`, `T`XXX, usage:
```lua

---@generic T
---@class aaa.`T`.bbb
---@return T
function get_type(a)
end

local d = get_type('xxx') --- aaa.xxx.bbb
```

`NEW` Support `@see` any thing

`NEW` Enhance module documentation export

`NEW` Support `@module` usage: `---@module "module path"`

# 0.5.4

`Fix` Fix generic dots params type check

# 0.5.3 

`NEW` Support negative integer as type

`Fix` Fix alias type check issue

`CHG` Refactor flow analyze algorithm

`FIX` Fix property unwrap issue

`NEW` Support filter the completion item

`NEW` Support reindex project when save a file

`NEW` Support check for `redundant_parameter`, `redundant_return_value`, `missing_return_value`, `return_type_mismatch`

`NEW` Better Support require module for other editor

`NEW` Support function stat inherit param type from `@field` annotation

# 0.5.2 

`CHG` Refactor `folding range`

`FIX` Fix super class completion issue

`NEW` Support `@field` function overload like:
```lua
---@class AAA
---@field event fun(s:string):string
---@field event fun(s:number):number
```

`FIX` Fix enum type check

`FIX` custom operator infer

`FIX` Fix select function and add std.Select type 

`CHG` Refactor Union type

`NEW` Add description to type

`NEW` Support description without '#' on multi union

`NEW` Add standard library translation

`NEW` Optimize inlay hint for parameter, if the parameter name is the same as the variable name, the parameter name will not be displayed

# 0.5.1 

`FIX` Fix issue `emmylua_ls` might not exit in unix.

`NEW` Support TypeScript-like type gymnastics

`FIX` Fix infinite recursion issue in alias generics.

`NEW` Improve reference search

`NEW` Refactor type check

`NEW` Optimize hover

`NEW` Optimize completion

`NEW` Support `pcall` return type and check

# 0.5.0

`NEW` Support type-check when casting tuples to arrays.

`NEW` Now autocompletion suggests function overloads.

`NEW` Improved completion for integer member keys.

`NEW` Infer value by reassign

`NEW` Improved analyze base control flow

`NEW` Improved hover for class

`NEW` Optimized semantic token

`NEW` Infer Some table array as tuple

`NEW` Infer `{ ... }` as array

`NEW` Semantic Model now is immutable

`FIX` Fix inference issue by resolving iteration order problem.

`FIX` Improve type check

# 0.4.6

`FIX` Fix issue with executable file directory hierarchy being too deep.

# 0.4.5

`FIX` Fix generic table infer issue

`FIX` Fix tuple infer issue

`NEW` Compact luals env variable start with `$`

`FIX` Refactor `humanize type` for stack overflow issue

`Fix` Fix a documentation cli tool render issue

`FIX` Fix diagnostic progress issue

# 0.4.4

`NEW` Support generic alias fold

`NEW` Support `code style check`, which powered by `emmyluacodestyle`

`NEW` Basic table declaration field names autocompletion.

`FIX` Fix possible panic due to integer overflow when calculating pows.

`NEW` Support compile by windows mingw

`NEW` `emmylua_check` now supports `workspace.library`

# 0.4.3

`FIX` Fix std resource loaded for cli tools

# 0.4.2

`FIX` Fix `self` parameter regard as unuseful issue

`NEW` Add `emmylua_check` cli tool, you can use it to check lua code. you can install it by `cargo install emmylua_check`

# 0.4.1 

`NEW` all the crates release to crates.io. now you can get `emmylua_parser`, `emmylua_code_analysis`, `emmylua_ls`, `emmylua_doc_cli` from crates.io.
```shell
cargo install emmylua_ls
cargo install emmylua_doc_cli
```

# 0.4.0 

`CHG` refactor `template system`, optimize the generic infer

`FIX` now configurations are loaded properly in NeoVim in cases when no extra LSP configuration parameters are provided

`CHG` extended humanization of small constant table types

`NEW` Add configuration option `workspace.moduleMap` to map old module names to new ones. The `moduleMap` is a list of mappings, for example:

```json
{
    "workspace": {
        "moduleMap": [
            {
                "pattern": "^lib(.*)$",
                "replace": "script$1"
            }
        ]
    }
}
```

This feature ensures that `require` works correctly. If you need to translate module names starting with `lib` to use `script`, add the appropriate mapping here.

`CHG` Refactor project structure, move all resources into executable binary

# 0.3.3 

`NEW` Add Develop Guide

`NEW` support `workspace/didChangeConfiguration` notification for neovim

`CHG` refactor `semantic token`

`NEW` support simple generic type instantiation based on the passed functions

`FIX` Fix find generic class template parameter issue

# 0.3.2

`FIX` Fixed some multiple return value inference errors

`FIX` Removed redundant `@return` in hover

`NEW` Language server supports locating resource files through the `$EMMYLUA_LS_RESOURCES` variable

# 0.3.1

`FIX` Fixed a potential issue where indexing could not be completed

`FIX` Fixed an issue where type checking failed when passing subclass parameters to a parent class

# 0.3.0

`NEW` Add progress notifications for non-VSCode platforms

`NEW` Add nix flake for installation by nix users, refer to PR#4

`NEW` Support displaying parameter and return descriptions in hover

`NEW` Support viewing consecutive require statements as import blocks, automatically folded by VSCode if the file only contains require statements

`FIX` Fix spelling error `interger` -> `integer`

`FIX` Fix URL parsing issue when the first directory under a drive letter is in Chinese

`FIX` Fix type checking issues related to tables

`FIX` Modify the implementation of document color, requiring continuous words, and provide an option to disable the document color feature

`FIX` Fix type inference issue when `self` is used as an explicit function parameter

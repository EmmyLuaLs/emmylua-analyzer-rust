# 📝 CHANGELOG

*All notable changes to the EmmyLua Analyzer Rust project will be documented in this file.*

---
## [0.8.2] - Unreleased

### ✨ Added
- **Support for Descriptions Above and After Tags**: You can now add descriptions both above a tag (as a preceding comment) and after a tag (inline). The description will be associated with the corresponding tag.
  ```lua
  ---@class A
  --- Description below (applies to the @field a)
  ---@field a integer inline-description
  ---@field b integer # description after hash
  --- Description above (applies to the @field b)
  ---@field c integer inline-description
  local a = {}
  ```

### 🔧 Changed
- **Class Method Completion**: When a function call jumps, if there are multiple declarations, It will then attempt to return the most matching definition along with all actual code declarations, rather than returning all definitions.

### 🐛 Fixed
- **Enum Variable Parameter Issue**: Fixed a crash issue when checking enum variable as parameter

## [0.8.1] - 2025-6-14

### 🔧 Changed
- **Generic constraint improvements**: Generic constraint (StrTplRef) removes the protection for string
  ```lua
  ---@generic T: string -- need to remove `: string`
  ---@param a `T`
  ---@return T
  local function class(a)
  end

  ---@class A
  local A = class("A") -- error
  ```

### ✨ Added
- **Immutable Tuples**: Explicitly declared `Tuple` are now immutable
  ```lua
  ---@type [1, 2]
  local a = {1, 2}
  a[1] = 3 -- error
  ```

- **Class Default Call Configuration**: Added `classDefaultCall` configuration item
  ```json
  {
    "runtime": {
      "classDefaultCall": {
        "functionName": "__init",
        "forceNonColon": true,
        "forceReturnSelf": true
      }
    }
  }
  ```

- **Base Type Matching**: Added `docBaseConstMatchBaseType` configuration item
  ```json
  {
    "strict": {
      "docBaseConstMatchBaseType": true
    }
  }
  ```

- **Enhanced Inlay Hints**: Params hint can now jump to the actual type definition
- **Improved File Management**: When closing files not in workspace/library, their impact is removed
- **Enhanced Ignore Functionality**: Ignored files won't be parsed when opened

### 🐛 Fixed
- **Function Hover**: Function hover now shows corresponding doc comments
- **Go to Definition**: Fixed crash when using "go to definition" of member
- **Enum Parameters**: Fixed enum usage as function parameters
- **Function Completion**: Fixed function completion for table fields expecting functions

---

## [0.8.0] - 2025-5-30

### ✨ Added
- **New Standard Types**: 
  - `std.Unpack` type for better `unpack` function inference
  - `std.Rawget` type for better `rawget` function inference
- **Generator Support**: Implementation similar to `luals`
- **Enhanced Generic Inference**: Improved generic parameter inference for lambda functions
- **Type Checking**: Added type checking for intersection types
- **Generic Constraints**: Support for generic constraint checking and string template parameters
- **Documentation Hints**: Added in code completion for modules and types

### 🔧 Changed
- **Math Library**: Changed `math.huge` to number type
- **Type Hints**: Optimized rendering of certain type hints

### 🐛 Fixed
- **Type Narrowing**: Fixed issue where type narrowing is lost in nested closures
- **Variadic Returns**: Optimized inference of variadic generic return values
- **Performance**: Fixed performance issue with large Lua tables causing unresponsiveness

---

## [0.7.3]

### ✨ Added
- **@return_cast Support**: Support `@return_cast` for functions. When a function's return value is boolean (must be annotated as boolean), you can add an additional annotation `---@return_cast <param> <cast op>`, indicating that when the function returns true, the parameter `<param>` will be transformed to the corresponding type according to the cast. For example:
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

### 🔧 Changed
- **Diagnostic Changes**: Remove diagnostic `lua-syntax-error`, it merges into `syntax-error`, add `doc-syntax-error` for doc syntax error
- **Format Changes**: Fix format issue, Now When exist `syntax-error`, the format never return value

### 🐛 Fixed
- **Performance Fixes**: Fix a performance issue: prevent large union types when functions return tables
- **Require Function Changes**: When an object returned by require function is a class/enum, defining new members on it is prohibited, while tables are not restricted
- **Lua 5.5 Support**: Support `Lua 5.5` global decl grammar
- **TypeGuard Support**: Support `TypeGuard<T>` as return type. For example:
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

---

## [0.7.2]

### ✨ Added
- **Call Hierarchy Support**: Support `Call hierarchy` but only support incomming call
- **@internal Tag**: Support new tag `@internal` for members or declarations. When a member or declaration is marked as `@internal`, it is only visible within its current library. This means that if you use `@internal` in one library, you cannot access this member or declaration from other libraries or workspace.
- **Go to Implementation**: Support `Go to implementation`
- **@nodiscard with Reason**: Support `@nodiscard` with reason

### 🐛 Fixed
- **Performance Fixes**: Fix Some performance issue

---

## [0.7.1]

### ✨ Added
- **Global Configuration Support**: Now language server configuration might be provided globally via the `<os-specific home dir>/.emmyrc.json`, `<os-specific config dir>/emmylua_ls/.emmyrc.json`, or by setting a variable `EMMYLUALS_CONFIG` with a path to the json configuration.
Global configuration have less priority than the local one
- **Class Inference from Generic Types**: Classes might now infer from generic types and provide corresponding completions.

### 🔧 Changed
- **Flow Analyze Algorithm**: Refactor flow analyze algorithm

### 🐛 Fixed
- **Self Inference**: Fix some self infer issue 
- **Diagnostic Action**: Fix some diagnostic action issue
- **Type Check and Completion**: Optimize some type check and completion

---

## [0.7.0]

### 🔧 Changed
- **Type Infer Refactor**: Refactor `type infer`
- **Member Infer Refactor**: Refactor `member infer`
- **Tuple Type Check**: Optimize and Fix tuple type check
- **Math Library**: Changed `math.huge` to number type

### ✨ Added
- **Variadic Type Support in Tuple**: Support Varidic type use in tuple, eg: `[string, integer...]`
- **Pcall Infer Optimization**: Optimize pcall infer, now can match the self and alias
- **Range Iter Var Optimization**: for range iter var now will remove nil type
- **Setmetatable Infer Support**: Support infer from setmetatable
- **emmylua_doc_cli Export**: emmylua_doc_cli will export more information
- **Subclass and Super Class Rule Optimization**: Optimize type check rule for subclass and super class
- **Description Support for Union Type**: Add description to type
- **Multi Union Description Support**: Support description without '#' on multi union
- **Standard Library Translation**: Add standard library translation
- **Parameter Inlay Hint Optimization**: Optimize inlay hint for parameter, if the parameter name is the same as the variable name, the parameter name will not be displayed

---

## [0.6.0]

### ✨ Added
- **Re-index Control**: Disable re-index in default, need to enable by `workspace.enableReindex`
- **New Diagnostics**: Add New Diagnostics `inject_field`, `missing_fields`, `redefined_local`, `undefined_field`, `inject-field`, `missing-global-doc`, 
`incomplete-signature-doc`, `circle-doc-class`, `assign-type-mismatch`, `unbalanced_assignments`, `check_return_count`, `duplicate_require`, `circle_doc_class`, `incomplete_signature_doc`, `unnecessary_assert`
- **Boolean Type Support**: Support `true` and `false` as type
- **Compact Fun Return Syntax**: Compact luals fun return syntax like: `(name: string, age: number)`
- **Iterator Function Aliases**: Aliases and overloads of iterator functions (i.e `fun(v: any): (K, V)` where `K` is the key type and `V` is the value type) are now used to infer types in `for` loops
- **Compact String Template Syntax**: Compact luals string template syntax like: xxx`T`, `T`, `T`XXX, usage:
  ```lua

  ---@generic T
  ---@class aaa.`T`.bbb
  ---@return T
  function get_type(a)
  end

  local d = get_type('xxx') --- aaa.xxx.bbb
  ```
- **@see Support**: Support `@see` any thing
- **Module Documentation Export Enhancement**: Enhance module documentation export
- **@module Support**: Support `@module` usage: `---@module "module path"`

### 🔧 Changed
- **Generic Dots Params Type Check**: Fix generic dots params type check

---

## [0.5.4]

### 🐛 Fixed
- **Generic Table Infer Issue**: Fix generic table infer issue
- **Tuple Infer Issue**: Fix tuple infer issue

### ✨ Added
- **Env Variable Support**: Compact luals env variable start with `$`
- **Humanize Type Refactor**: Refactor `humanize type` for stack overflow issue
- **Documentation CLI Tool Render Enhancement**: Fix a documentation cli tool render issue
- **Diagnostic Progress Issue Fix**: Fix diagnostic progress issue

---

## [0.5.3]

### ✨ Added
- **Negative Integer Type Support**: Support negative integer as type
- **TypeScript-like Type Gymnastics**: Support TypeScript-like type gymnastics
- **Reference Search Improvement**: Improve reference search
- **Type Check Refactor**: Refactor type check
- **Hover Optimization**: Optimize hover
- **Completion Optimization**: Optimize completion
- **Pcall Return Type Support**: Support `pcall` return type and check

### 🐛 Fixed
- **Infinite Recursion Issue in Alias Generics**: Fix infinite recursion issue in alias generics.

---

## [0.5.2]

### ✨ Added
- **Fold Range Refactor**: Refactor `folding range`
- **Super Class Completion Support**: Fix super class completion issue
- **Function Overload Support in @field**: Support `@field` function overload like:
  ```lua
  ---@class AAA
  ---@field event fun(s:string):string
  ---@field event fun(s:number):number
  ```
- **Enum Type Check Fix**: Fix enum type check
- **Custom Operator Infer Fix**: Fix custom operator infer
- **Select Function Fix and Std.Select Type Addition**: Fix select function and add std.Select type 
- **Union Type Refactor**: Refactor Union type
- **Description Support for Type**: Add description to type
- **Multi Union Description Support**: Support description without '#' on multi union
- **Standard Library Translation**: Add standard library translation
- **Parameter Inlay Hint Optimization**: Optimize inlay hint for parameter, if the parameter name is the same as the variable name, the parameter name will not be displayed

---

## [0.5.1]

### 🐛 Fixed
- **Unix Issue Fix**: Fix issue `emmylua_ls` might not exit in unix.

### ✨ Added
- **TypeScript-like Type Gymnastics**: Support TypeScript-like type gymnastics
- **Reference Search Improvement**: Improve reference search
- **Type Check Refactor**: Refactor type check
- **Hover Optimization**: Optimize hover
- **Completion Optimization**: Optimize completion
- **Pcall Return Type Support**: Support `pcall` return type and check

---

## [0.5.0]

### ✨ Added
- **Tuple to Array Casting Type-check**: Support type-check when casting tuples to arrays.
- **Function Overloads Autocompletion**: Now autocompletion suggests function overloads.
- **Improved Completion for Integer Member Keys**: Improved completion for integer member keys.
- **Value Inference by Reassign**: Infer value by reassign
- **Base Control Flow Analyze Improvement**: Improved analyze base control flow
- **Class Hover Enhancement**: Improved hover for class
- **Semantic Token Optimization**: Optimized semantic token
- **Tuple Inference for Table Array**: Infer Some table array as tuple
- **Array Inference for `{ ... }`**: Infer `{ ... }` as array
- **Immutable Semantic Model**: Semantic Model now is immutable

### 🐛 Fixed
- **Iteration Order Issue**: Fix inference issue by resolving iteration order problem.
- **Type Check Improvement**: Improve type check

---

## [0.4.6] 

### 🐛 Fixed
- **Executable File Directory Hierarchy Issue**: Fix issue with executable file directory hierarchy being too deep.

---

## [0.4.5]

### 🐛 Fixed
- **Generic Table Infer Issue**: Fix generic table infer issue
- **Tuple Infer Issue**: Fix tuple infer issue

### ✨ Added
- **Env Variable Support**: Compact luals env variable start with `$`
- **Humanize Type Refactor**: Refactor `humanize type` for stack overflow issue
- **Documentation CLI Tool Render Enhancement**: Fix a documentation cli tool render issue
- **Diagnostic Progress Issue Fix**: Fix diagnostic progress issue

---

## [0.4.4]

### ✨ Added
- **Generic Alias Fold Support**: Support generic alias fold
- **Code Style Check**: Support `code style check`, which powered by `emmyluacodestyle`
- **Basic Table Declaration Field Names Autocompletion**: Basic table declaration field names autocompletion.

### 🐛 Fixed
- **Integer Overflow Panic Issue**: Fix possible panic due to integer overflow when calculating pows.

---

## [0.4.3]

### 🐛 Fixed
- **Std Resource Loaded for CLI Tools**: Fix std resource loaded for cli tools

---

## [0.4.2]

### 🐛 Fixed
- **Self Parameter Regard as Unuseful Issue**: Fix `self` parameter regard as unuseful issue

### ✨ Added
- **emmylua_check CLI Tool**: Add `emmylua_check` cli tool, you can use it to check lua code. you can install it by `cargo install emmylua_check`

---

## [0.4.1]

### ✨ Added
- **Global Crates Release**: all the crates release to crates.io. now you can get `emmylua_parser`, `emmylua_code_analysis`, `emmylua_ls`, `emmylua_doc_cli` from crates.io.
  ```shell
  cargo install emmylua_ls
  cargo install emmylua_doc_cli
  ```

---

## [0.4.0]

### 🔧 Changed
- **Template System Refactor**: refactor `template system`, optimize the generic infer
- **Configuration Loading in NeoVim**: now configurations are loaded properly in NeoVim in cases when no extra LSP configuration parameters are provided
- **Humanization of Small Constant Table Types**: extended humanization of small constant table types

### ✨ Added
- **Module Name Mapping Configuration**: Add configuration option `workspace.moduleMap` to map old module names to new ones. The `moduleMap` is a list of mappings, for example:

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

- **Project Structure Refactor**: Refactor project structure, move all resources into executable binary

---

## [0.3.3]

### ✨ Added
- **Develop Guide**: Add Develop Guide
- **workspace/didChangeConfiguration Notification Support**: support `workspace/didChangeConfiguration` notification for neovim
- **Semantic Token Refactor**: refactor `semantic token`
- **Simple Generic Type Instantiation Support**: support simple generic type instantiation based on the passed functions
- **Find Generic Class Template Parameter Issue Fix**: Fix find generic class template parameter issue

---

## [0.3.2]

### 🐛 Fixed
- **Multiple Return Value Inference Errors**: Fixed some multiple return value inference errors
- **Redundant @return in Hover**: Removed redundant `@return` in hover

### ✨ Added
- **Resource File Location Support**: Language server supports locating resource files through the `$EMMYLUA_LS_RESOURCES` variable

---

## [0.3.1]

### 🐛 Fixed
- **Indexing Completion Issue**: Fixed a potential issue where indexing could not be completed
- **Type Checking with Subclass Parameters**: Fixed an issue where type checking failed when passing subclass parameters to a parent class

---

## [0.3.0]

### ✨ Added
- **Progress Notifications for Non-VSCode Platforms**: Add progress notifications for non-VSCode platforms
- **Nix Flake Installation Support**: Add nix flake for installation by nix users, refer to PR#4
- **Parameter and Return Descriptions in Hover**: Support displaying parameter and return descriptions in hover
- **Consecutive Require Statements as Import Blocks**: Support viewing consecutive require statements as import blocks, automatically folded by VSCode if the file only contains require statements

### 🐛 Fixed
- **Spelling Error**: Fix spelling error `interger` -> `integer`
- **URL Parsing Issue**: Fix URL parsing issue when the first directory under a drive letter is in Chinese
- **Table Type Checking Issues**: Fix type checking issues related to tables
- **Document Color Implementation**: Modify the implementation of document color, requiring continuous words, and provide an option to disable the document color feature
- **Type Inference Issue with Self as Parameter**: Fix type inference issue when `self` is used as an explicit function parameter

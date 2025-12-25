# 📝 CHANGELOG

*All notable changes to the EmmyLua Analyzer Rust project will be documented in this file.*

## [0.19.0] - 2025-12-25

### ✨ Added
- **Support Lua5.5**: Added support for Lua 5.5 syntax and features, including global declarations, table.create, and named vararg. For example:
```lua
global *
global <const> a, b, c
global d, e, f = 1, 2, 3
table.create(10, 0)
function func(...args)
end
```

- **Support format Lua 5.5 syntax**: The built-in formatter now supports formatting Lua 5.5 syntax.
- **Add new stdlib i18n translation**: Added new internationalization functions to the standard library.
- **Support call argument snippet completion**: When `"completion.callSnippet": true` is enabled, provide snippet completions for function arguments during function calls.
- **Support param/@return completion**: Typing `---@` above a function will show `param/@return` completion suggestions; accepting a suggestion will automatically fill parameter names and types.

### 🔧 Changed

- **Workspace variable search optimization**: Optimized workspace-wide variable search to decide whether to use case-sensitive or case-insensitive matching based on the input's casing.

### 🐛 Fixed

- **Fix integer literal parsing issue**: Integers exceeding int64 are now recognized as floats instead of being treated as 0.
- **Fix typecheck**: Fixed several type checking issues.

## [0.18.0] - 2025-12-5

An experimental Lua 5.4 interpreter implemented in Rust: https://github.com/CppCXY/lua-rs

### ✨ Added

- **Type narrowing with union types using field checks**: Changed the behavior when using a field of a union type in an `if` statement for type narrowing. Now, if the field exists in some types of the union but not in others, the types without that field will be excluded from the narrowed type. For example:
```lua
local a --- @type string|{foo:boolean, bar:string}

if a.foo then
  local _ = a.bar -- a will be narrowed to {foo:boolean, bar:string}
end
```
- **Support generic in @field**: You can now use declaration generic type in `@field` annotations. For example:
```lua
---@class GetType
---@field get_type fun<T>(name:`T`): T
local MyClass = {}

local d = MyClass.get_type("Car") -- d: "Car"
```

### 🔧 Changed

- **Refactor Document Symbols**: Refactored the `textDocument/documentSymbol` request to improve performance and accuracy. The new implementation provides better handling of nested symbols and improves the overall structure of the returned symbol tree.


### 🐛 Fixed
- **Fix Lazyvim.dev integration issue**: Fixed an issue where Lazyvim.dev integration did not work correctly due to ignore `workspace/didConfiguration` changes.
- **Fix Completion**: Fixed an issue where certain completions were not being suggested, like:
`Partial<Type>`
- **Fix nil propagation in consecutive field access**: Fixed an issue where, during consecutive field access, if a previous field could be nil, subsequent fields would incorrectly propagate the nil type. For example:
```lua
local a --- @type { foo? : { bar: { baz: number } } }

local b = a.foo.bar -- a.foo may be nil (correct)

local _ = b.baz -- b is number
```



---
## [0.17.0] - 2025-11-7
### 🔧 Changed
- **Refactor IndexAliasName**: Removed the original index alias implementation (`-- [IndexAliasName]`), now use `---@[index_alias("name")]`.
- **Refactor ClassDefaultCall**: Removed the configuration item `runtime.class_default_call`, now use `---@[constructor("<constructor_method_name>")]`.
- **Rename ParamTypeNotMatch to ParamTypeMismatch**: Renamed the diagnostic `ParamTypeNotMatch` to `ParamTypeMismatch` for better clarity.
- **Optimize comment parsing logic**: Comments now preserve leading spaces at the start of each line, maintaining the original formatting as much as possible when returned to the LSP client.


### ✨ Added
- **Attribute**: Introduced the new feature `---@attribute` for defining additional metadata, with several built-in attributes:
```lua
--- Deprecated. Accepts an optional message parameter.
---@attribute deprecated(message: string?)

--- Language Server Optimization Items.
---
--- Parameters:
--- - `check_table_field`: Skips assignment checks for table fields. Recommended for large configuration tables.
--- - `delayed_definition`: Indicates the variable type is determined by the first assignment.
---   Only valid for `local` declarations without an initial value.
---@attribute lsp_optimization(code: "check_table_field"|"delayed_definition")

--- Index field alias, displayed in `hint` and `completion`.
---
--- Accepts a string parameter for the alias name.
---@attribute index_alias(name: string)

--- This attribute must be applied to function parameters, and the parameter type must be a string template generic.
--- Used to specify the default constructor of a class.
---
--- Parameters:
--- - `name`: The method name as a constructor.
--- - `root_class`: Marks the root class, will be implicitly inherited, e.g., `System.Object` in C#. Defaults to empty.
--- - `strip_self`: Whether the `self` parameter can be omitted when calling the constructor, defaults to `true`.
--- - `return_self`: Whether the constructor is forced to return `self`, defaults to `true`.
---@attribute constructor(name: string, root_class: string?, strip_self: boolean?, return_self: boolean?)

--- Associates `getter` and `setter` methods with a field. Currently only provides definition navigation functionality.
--- The target methods must be within the same class.
---
--- Parameters:
--- - `convention`: Naming convention, defaults to `camelCase`. Implicitly adds `get` and `set` prefixes. e.g., `_age` -> `getAge`, `setAge`.
--- - `getter`: Getter method name. Takes precedence over `convention`.
--- - `setter`: Setter method name. Takes precedence over `convention`.
---@attribute field_accessor(convention: "camelCase"|"PascalCase"|"snake_case"|nil, getter: string?, setter: string?)
```

The syntax is `---@[attribute_name_1(arg...), attribute_name_2(arg...), ...]`, and multiple attributes can be used simultaneously. Example:
```lua
---@class A
---@[deprecated] # If the attribute can omit parameters, `()` can be omitted
---@field b string # b is now marked as deprecated
---@[index_alias("b")]
---@field [1] string # This will be shown as `b` in hints and completion
```
- **More Generic Type**: support generic like:
```lua
--- Get the parameters of a function as a tuple
---@alias Parameters<T extends function> T extends (fun(...: infer P): any) and P or never

--- Get the parameters of a constructor as a tuple
---@alias ConstructorParameters<T> T extends new (fun(...: infer P): any) and P or never

--- Make all properties in T optional
---@alias Partial<T> { [P in keyof T]?: T[P]; }
```

- **Support gutter request for intellij**: Added support for gutter requests in IntelliJ, allowing for better integration with the IDE's features.

### 🐛 Fixed
- **Fix completion**: Fixed an issue where certain completions were not being suggested, like:
```lua
if not self:<|>
```
- **Fix '~' Replace error in config**: Fixed an issue where using '~' in configuration paths did not correctly expand to the user's home directory.
- **Fix enum completion issue**: Fixed an issue where enum members were not being suggested in completions.
- **Fix workspace load status bar**: Fixed an issue where the workspace load status bar always display in empty lua workspace.
- **Fix some condition narrow**: Fixed some issues with condition-based type narrowing not working as expected.


## [0.16.0] - 2025-10-17
### ✨ Added
- **Support `workspace/diagnostic`**: Added support for the `workspace/diagnostic` request, allowing clients to fetch diagnostics for the entire workspace.
- **Support global function overload**: You can now define overloads for global functions in `@meta` files. For example:
```lua
---@meta


function overload(a: integer): integer
end

function overload(a: string): string
end
```

### 🔧 Changed
- **Update lsp-server dependency**: Updated the `lsp-server` dependency to version 0.7.9 to leverage the latest features and improvements.
- **Migrate `lsp-types` to `emmy-lsp-types`**: Migrated from using the `lsp-types` crate to the `emmy-lsp-types` crate, which is a fork tailored for EmmyLua Analyzer Rust. This change allows for better customization and alignment with the project's specific needs.

### 🐛 Fixed
- **Fix deadlock issue**: Resolved a deadlock issue that could occur during certain operations, improving the stability of the language server.
- **Fix diagnostic reporting**: Fixed an issue where some diagnostics never cleaned up after files were change information.
- **Fix overload description issue**: Fixed an issue where descriptions for overloaded functions were not displayed correctly in completion and hover tooltips.

## [0.15.0] - 2025-10-10

### ✨ Added
- **Use Clippy as linter**: core codebase now uses Clippy as the linter, improving code quality and consistency.
- **Support `textdocument/diagnostic`**: Added support for the `textDocument/diagnostic` request, allowing clients to fetch diagnostics for a specific document.
- **Support annotation `@readonly`**: You can now use the `@readonly` annotation to mark fields as read-only. For example:
```lua
---@readonly
local myVar = 42
```
- **Add check for `global in non module`**: Added a new diagnostic to check for global variable declarations in non-module scope. This helps detect unintended global variable declarations.

### 🔧 Changed
- **Optimize semantic token**: Optimized semantic token handling for delimiter symbols.

### 🐛 Fixed
- **Fix generic pattern matching issue**: Fixed an issue where generic pattern matching aliases could lead to incorrect type inference.

## [0.14.0] - 2025-9-19

### 🔧 Changed

- **Parser Optimization**: The parser now reports syntax errors more accurately and has improved error recovery.
- **@type Support for Return Statements**: You can now use `@type` above a return statement to specify the return value type, for example:
```lua
---@return vim.lsp.Config
return {}
```
- **Type Checking Optimization**: Improved type checking algorithms for better performance.

### ✨ Added
- **SARIF Format for emmyLua_check**: `emmyLua_check` now supports SARIF format output, enabled via the `--format sarif` command line option.
- **Generic List Supports T... Syntax**: Generic lists now support the `T...` syntax, for example:
```lua
---@alias MyTuple<T...> [T...]
```

### 🐛 Fixed
- **Fix create progress**: Fixed an issue with the `window/workDoneProgress/create` protocol; it must be sent as a request, not a notification.
- **Fix Function Overload Algorithm**: Rewrote the function overload algorithm to better handle variadic function parameters.


## [0.13.0] - 2025-9-9

### 🐛 Fixed
- **LSP Handler Order**: Fixed an issue where LSP request donot handle during initialization.it will be handle after initialization complete.

### ✨ Added
- **Support @link in comment**: You can now use `@link` in comments to create clickable links. For example:
```lua
--- This is a link to {@link string.format}
```
- **Support `--editor` directive**: You can now use the `--editor` directive to specify the editor type. For example:
```shell
emmylua_ls --editor intellij
```
- **Support range foramt for external tool**: You can now use the `rangeFormat` request to format a specific range of code using an external tool. This feature can be enabled with the following configuration:
```json
{
  "format": {
    "externalToolRangeFormat": {
        "program": "stylua",
        "args": [
            "-",
            "--stdin-filepath",
            "${file}",
            "--indent-width=${indent_size}",
            "--indent-type",
            "${use_tabs?Tabs:Spaces}",
            "--range-start=${start_offset}",
            "--range-end=${end_offset}"
        ],
        "timeout": 5000
    }
  }
}
```
for more information, please refer to [External Formatter Options](docs/external_format/external_formatter_options_EN.md).
- **Add Basic EmmyLua Annotation Documentation**: Added more documentation for EmmyLua annotations, please refer to [EmmyLua Annotation Documentation](docs/emmylua_doc/annotations_EN/README.md).


### 🔧 Changed
- **Refactor LSP Handler**: Refactored LSP handler to improve performance and maintainability.
- **Refactor Folding Range**: Refactored folding range to support `Intellij`
- **Add More Semantic Token**: Added more semantic tokens to improve syntax highlighting.

## [0.12.0] - 2025-8-22

### 🐛 Fixed
- **Crash issue fixed**: Fixed a crash caused by parsing Unicode characters in comments.
- **Large table performance issue fixed**: Fixed a performance issue where parsing large array tables in projects caused severe slowdowns.
- **Generic type matching fixed**: Fixed an issue with incorrect matching of `constTpl<T>` types affecting generic type hints.

### ✨ Added
- **Markdown syntax highlighting enabled by default**: Markdown syntax highlighting in comments is now enabled by default, including partial syntax highlighting for code blocks within comments.
- **Support for `@language`**: Added support for using `@language` in comments to specify the language of code blocks, for example:
  ```lua
  ---@language lua
  local d = [[
    print("Hello, world!")
  ]]
  ```
  This enables syntax highlighting for Lua code.

- **Support for `Language<T>` generic type**: You can now use `Language<T: string>` in parameter comments to specify the language of a parameter, for example:
  ```lua
  ---@param lang Language<"vim">
  function vim_run(lang)
  end

  vim_run [[set ft=lua]]
  ```
  Supported injected languages: lua, vim, sql, json, shell, protobuf.

- **Support for `keyof type`**: When a function parameter is `keyof type`, corresponding code completion is provided.

## [0.11.0] - 2025-8-8

### 🐛 Fixed
- **Fixed a stack overflow crash**: Resolved an issue that caused the language server to crash due to excessive recursion.
- **Fixed a deadlock issue**: Resolved an issue that caused the language server to hang indefinitely in Neovim.
- **Fixed workspace libraries**: Resolved an issue where libraries in subdirectories were incorrectly added to the main workspace.
- **Fixed error reporting**: Resolved an issue where error reports were not being generated correctly for table fields.

### ✨ Added
- **Support for Markdown/MarkdownRst**: Added support for Markdown and reStructuredText (RST) formats highlighted in documentation comments.
This feature is disabled by default and can be enabled with the following configuration:
```json
{
  "semanticTokens": {
    "renderDocumentationMarkup": true
  },
  "doc": {
    "syntax": "md"
  }
}
```

- **Support for external formatting tools**: Added support for external formatting tools. You can now configure an external formatter to format your Lua code. This feature can be enabled with the following configuration:
```json
{
  "format": {
    "externalTool": {
      "program": "stylua",
      "args": [
        "-",
        "--stdin-filepath",
        "${file}",
        "--indent-width=${indent_size}",
        "--indent-type",
        "${use_tabs:Tabs:Spaces}"
      ]
    }
  }
}
```
Note: The built-in formatter is not stylua, but emmyluacodestyle. This feature simply provides an extension point, allowing users to use their preferred formatting tool. In terms of performance, using this extension may be faster than using other plugins.

- **Support for non-standard symbols**: Added support for non-standard symbols in Lua.

```json
{
  "runtime": {
    "nonstandardSymbol": [
      "//",
      "/**/",
      "`",
      "+=",
      "-=",
      "*=",
      "/=",
      "%=",
      "^=",
      "//=",
      "|=",
      "&=",
      "<<=",
      ">>=",
      "||",
      "&&",
      "!",
      "!=",
      "continue"
    ]
  }
}
```



## [0.10.0] - 2025-7-27
### 🐛 Fixed
- **Fix create an empty directory**:  Fixed an issue where the language server would create an empty directory.
### 🔧 Changed
- **Rust Edition 2024**: The language server is now built with Rust Edition 2024, which brings various performance and stability improvements.


## [0.9.1] - 2025-7-25
### 🔧 Changed
- **Refactor generic function inference**: Lambda function parameters now use deferred matching, allowing generic types to be inferred from other parameters first. For example:
```lua
---@generic T
---@param f1 fun(...: T...): any
---@param ... T...
function invoke(f1, ...)

end

invoke(function(a, b, c) -- infer as: integer, integer, integer
    print(a, b, c)
end, 1, 2, 3)
```

- **Generic Type Decay**: Now, generic types that match constants of integer, string, float, or boolean will be directly converted to their corresponding general types.

### ✨ Added
- **Use Mimalloc**: Mimalloc is now the default memory allocator, improving performance and memory management. Startup performance is increased by about 50%.
- **Lua 5.5 Syntax Support**: More complete support for Lua 5.5 syntax, including `global` declarations, `table.create`, and the new attribute syntax. For example:
```lua
local <const> a, b, c = 1, 2, 3
global <const> d, e, f
```
Also supports immutability checks for iterator variables in for loop statements.


- **Doc Cli Modification**: Improved the documentation CLI to better handle various edge cases and provide more accurate suggestions.

### 🐛 Fixed

- **Fix load order**: Fixed an issue where the order of loading files could lead to incorrect type inference.
- **Fix Unpack infer**: Fixed an issue where unpacking a table in a table.
- **Fix rename in @param**: Fixed an issue where renaming a parameter in a function param.

## [0.9.0] - 2025-7-11
### 🔧 Changed

- **Flow Inference Refactor**: Refactored flow analysis algorithm, now uses a TypeScript-like flow analysis approach for better handling of complex scenarios.
- **Doc CLI**: Changed export format, now supports multiple `@see` and other tag flags.

### ✨ Added

- **TypeGuard Now Supports Generics**: You can now use generic parameters in TypeGuard, e.g. `TypeGuard<T>`.
- **Type Narrowing by Constant Fields**: Supports type narrowing using constant fields.
- **Basic Range Checking**: Array type indexing is now less frequently nullable.

### 🐛 Fixed

- **Bug Fixes**: Fixed various bugs.

## [0.8.2] - 2025-6-27
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
- **Add call `__call` hint**: Add call `__call` hint, enable by `hint.metaCallHint`
  ```lua
  ---@class A
  ---@overload fun(a: integer): integer
  local A
  A(1) -- There will be a lightning prompt between `A` and `(` or a `new` prompt before `A`
  ```

- **Support syntax`--[[@cast -?]]`**: When `@cast` is followed by an operator instead of a name, it will convert the type of the previous expression, but currently only works for function calls!

- **Quick Fix for Nil Removal**: Added quick fix action for `NeedCheckNil` diagnostic that suggests using `@cast` to remove nil type
  ```lua
  ---@Class Cast1
  ---@field get fun(self: self, a: number): Cast1?
  local A

  local _a = A:get(1) --[[@cast -?]]:get(2):get(3) -- Quick fix will prompt whether to automatically add `--[[@cast -?]]`
  ```
- **Base Function Name Completion**: Added `completion.baseFunctionIncludesName` configuration to control whether function names are included in base function completions
  ```json
  {
    "completion": {
      "baseFunctionIncludesName": true
    }
  }
  ```
  When enabled, function completions will include the function name: `function name() end` instead of `function () end`

- **Cast Type Mismatch Diagnostic**: Added new diagnostic `CastTypeMismatch` to detect type mismatches in cast operations
  ```lua
  ---@type string
  local a = "hello"
  --[[@cast a int]] -- Warning
  ```

- **Auto Require Naming Convention Configuration**: Added `completion.autoRequireNamingConvention.keep-class` configuration option. When importing modules, if the return value is a class definition, the class name will be used; otherwise, the file name will be used
  ```json
  {
    "completion": {
      "autoRequireNamingConvention": "keep-class"
    }
  }
  ```

- **File rename prompts whether to update `require` paths**: Added prompt when renaming files to ask whether to update corresponding import statements


### 🔧 Changed
- **Class Method Completion**: When a function call jumps, if there are multiple declarations, It will then attempt to return the most matching definition along with all actual code declarations, rather than returning all definitions.

- **Definition Jump Enhancement**: When jumping to definition from function calls, if the target is located in a return statement, the language server will now attempt to find the original definition. For example:
  ```lua
  -- test.lua
  local function test()
  end
  return {
      test = test,
  }
  ```
  ```lua
  local t = require("test")
  local test = t.test -- Previously jumped to: test = test,
  test() -- Now jumps to: local function test()
  ```

### 🐛 Fixed
- **Enum Variable Parameter Issue**: Fixed a crash issue when checking enum variable as parameter
- **Circle Doc Class Issue**: Fixed a bug that caused the language server to hang when


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

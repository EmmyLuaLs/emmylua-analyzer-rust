# CHANGELOG

# 0.5.3 (unreleased)

`NEW` Support negative integer as type

`Fix` Fix alias type check issue

`CHG` Refactor flow analyze algorithm

`FIX` Fix property unwrap issue

`NEW` Support filter the completion item

`NEW` Support reindex project when save a file

`NEW` Support check for `redundant_parameter`, `redundant_return_value`, `missing_return_value`, `return_type_mismatch`

`NEW` Better Support require module for other editor

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

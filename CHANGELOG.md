# CHANGELOG

# 0.4.5(unreleased)

`FIX` Fix generic table infer issue

`FIX` Fix tuple infer issue

`NEW` Compact luals env variable start with `$`

`FIX` Refactor `humanize type` for stack overflow issue

`Fix` Fix a documentation cli tool render issue

`NEW` Constant array tables are now inferred as tuples of elements

# 0.4.4

`NEW` Support generic alias fold

`NEW` Support `code style check`, which powered by `emmyluacodestyle`

`NEW` Basic table declaration field names autocompletion.

`FIX` Fix possible panic due to integer overflow when calculating pows.

`NEW` Support compile by winodws mingw

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

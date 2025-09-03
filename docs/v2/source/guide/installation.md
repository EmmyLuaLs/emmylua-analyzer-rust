# Installation

## VSCode

Install the [EmmyLua Extension] for the best development experience.

[EmmyLua Extension]: https://marketplace.visualstudio.com/items?itemName=tangzx.emmylua

## Intellij Idea

Install the [EmmyLua2 Plugin] from the JetBrains Marketplace.

[EmmyLua2 Plugin]: https://plugins.jetbrains.com/plugin/25076-emmylua2

## NeoVim

1. Install EmmyLua to your system using `cargo`
   (or another method, [see below](#standalone)):

   ```sh
   cargo install emmylua_ls
   ```

2. Install [nvim-lspconfig].

   [nvim-lspconfig]: https://github.com/neovim/nvim-lspconfig?tab=readme-ov-file#install

3. Enable LSP client in your `init.lua`:

   ```lua
   vim.lsp.enable({"emmylua_ls"})
   ```

4. You can [configure](./configuration.md) EmmyLua via `vim.lsp.config`:

   ```lua
   vim.lsp.config('emmylua_ls', {
     settings = {
       emmylua = {
         -- Anything from `.emmyrc.json`, for example:
         hint = {
           paramHint = false
         }
       }
     }
   })
   ```

   See [`:help lspconfig-all | /emmylua_ls`][nvim-help] for more info.

   [nvim-help]: https://github.com/neovim/nvim-lspconfig/blob/master/doc/configs.md#emmylua_ls

   :::{note}
     These options will override project-specific `.emmyrc.json`.

     For options that control LSP features, this is a desired behavior;
     for options that control language analysis, it is not.

     If you need to alter defaults without overriding project-specific settings,
     you can do so by creating `.emmyrc.json` in your home
     or config directory. See [](configuration.md#user-settings) for details.
   :::

## Standalone

EmmyLua is available as a standalone executable.

**Via Cargo:**

1. [Install Rust and Cargo].

2. ```sh
   # Install the language server
   cargo install emmylua_ls

   # Install documentation generator
   cargo install emmylua_doc_cli

   # Install static analyzer
   cargo install emmylua_check
   ```

[Install Rust and Cargo]: https://doc.rust-lang.org/cargo/getting-started/installation.html

**Pre-built binaries:**

Download the latest binaries from our [releases page].

[releases page]: https://github.com/CppCXY/emmylua-analyzer-rust/releases

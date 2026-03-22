<div align="center">

# EmmyLua Analyzer Rust

**A high-performance Lua language server, linter, and documentation generator — built with Rust.**

[![GitHub stars](https://img.shields.io/github/stars/CppCXY/emmylua-analyzer-rust?style=flat-square&logo=github)](https://github.com/CppCXY/emmylua-analyzer-rust/stargazers)
[![License](https://img.shields.io/github/license/CppCXY/emmylua-analyzer-rust?style=flat-square)](https://github.com/CppCXY/emmylua-analyzer-rust/blob/main/LICENSE)
[![Release](https://img.shields.io/github/v/release/CppCXY/emmylua-analyzer-rust?style=flat-square)](https://github.com/CppCXY/emmylua-analyzer-rust/releases)
[![Crates.io](https://img.shields.io/crates/d/emmylua_ls?style=flat-square&logo=rust)](https://crates.io/crates/emmylua_ls)

[Quick Start](#quick-start) · [Features](#features) · [Documentation](#documentation) · [Development](#development)

</div>

---

## Why EmmyLua Analyzer Rust?

- **Fast** — Incremental analysis powered by Rust; handles large codebases with ease
- **Complete** — Supports Lua 5.1 – 5.5 and LuaJIT, with EmmyLua & Luacats annotations
- **Universal** — Standard LSP protocol works with VS Code, Neovim, IntelliJ, and any LSP-compatible editor
- **All-in-one** — Language server, code formatter, static analyzer, and doc generator in a single toolchain

---

## Quick Start

### Install

```bash
# Via Cargo
cargo install emmylua_ls          # Language server
cargo install emmylua_check       # Static analyzer / linter
cargo install emmylua_doc_cli     # Documentation generator
```

Or download pre-built binaries from the [Releases](https://github.com/CppCXY/emmylua-analyzer-rust/releases) page.

### Editor Setup

<details>
<summary><b>VS Code</b></summary>

Install the [EmmyLua Extension](https://marketplace.visualstudio.com/items?itemName=tangzx.emmylua).

</details>

<details>
<summary><b>Neovim</b></summary>

```lua
vim.lsp.enable({"emmylua_ls"})
```

</details>

<details>
<summary><b>IntelliJ IDE</b></summary>

Install the [EmmyLua2 Plugin](https://plugins.jetbrains.com/plugin/25076-emmylua2) from the JetBrains Marketplace.

</details>

<details>
<summary><b>Other editors</b></summary>

Any editor with LSP support can use `emmylua_ls` via stdio (default) or TCP.

</details>

---

## Features

### Language Support

| Version | Status |
|---------|--------|
| Lua 5.1 | ✅ Full support |
| Lua 5.2 | ✅ Full support |
| Lua 5.3 | ✅ Integer types, UTF-8 |
| Lua 5.4 | ✅ Attributes, generational GC |
| Lua 5.5 | ✅ New global syntax |
| LuaJIT  | ✅ FFI, bit operations |

### LSP Capabilities

Completion · Go to Definition · Find References · Go to Implementation · Hover · Signature Help · Rename · Code Actions · Diagnostics · Document & Workspace Symbols · Formatting · Folding · Document Links · Semantic Tokens · Inlay Hints · Document Highlights · Code Lens · Call Hierarchy · Document Color

### Code Quality

- Static analysis with 40+ diagnostic rules
- Code formatting and style enforcement
- EmmyLua / Luacats annotation support
---

## Usage

### Language Server

```bash
# Default stdio mode
emmylua_ls

# TCP mode for remote debugging
emmylua_ls -c tcp --port 5007 --log-level debug --log-path ./logs
```

| Parameter | Description |
|-----------|-------------|
| `-c, --communication` | `stdio` (default) or `tcp` |
| `--port` | TCP port (default: 5007) |
| `--log-level` | `debug` / `info` / `warn` / `error` |
| `--log-path` | Log output directory |

### Static Analyzer

```bash
emmylua_check .                           # Analyze current directory
emmylua_check ./src --verbose --format json  # Detailed JSON output
```

### Documentation Generator

```bash
emmylua_doc_cli ./src --output ./docs
```

---

## Documentation

- [**Features Guide**](./docs/features/features_EN.md) - Comprehensive feature documentation
- [**Configuration**](./docs/config/emmyrc_json_EN.md) - Advanced configuration options
- [**Formatter Guide**](./docs/emmylua_formatter/README_EN.md) - Formatter behavior, options, and usage guide
- [**Annotations Reference**](./docs/emmylua_doc/annotations_EN/README.md) - Detailed annotation documentation
- [**Old Formatter**](https://github.com/CppCXY/EmmyLuaCodeStyle/blob/master/README_EN.md) - Formatting and style guidelines
- [**External Formatter Integration**](./docs/external_format/external_formatter_options_EN.md) - Using external formatters

---

## Development

### Build from Source

```bash
git clone https://github.com/EmmyLuaLs/emmylua-analyzer-rust.git
cd emmylua-analyzer-rust

cargo build --release              # Build everything
cargo build --release -p emmylua_ls   # Build only the language server
```

### Test

```bash
cargo test                      # Run all tests
cargo test -p emmylua_parser    # Run parser tests only
```

### Contributing

We welcome contributions! See [CONTRIBUTING.md](./CONTRIBUTING.md) for details.

---

## License

[MIT](./LICENSE)

---

<div align="center">

*Thanks to all contributors and the Lua community.*

[Back to top](#emmylua-analyzer-rust)

</div>

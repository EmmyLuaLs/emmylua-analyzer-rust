<div align="center">

# 🌟 EmmyLua Analyzer Rust

[![GitHub stars](https://img.shields.io/github/stars/CppCXY/emmylua-analyzer-rust?style=for-the-badge&logo=github&color=gold)](https://github.com/CppCXY/emmylua-analyzer-rust/stargazers)
[![GitHub license](https://img.shields.io/github/license/CppCXY/emmylua-analyzer-rust?style=for-the-badge&logo=mit&color=blue)](https://github.com/CppCXY/emmylua-analyzer-rust/blob/main/LICENSE)
[![GitHub release](https://img.shields.io/github/v/release/CppCXY/emmylua-analyzer-rust?style=for-the-badge&logo=github&color=green)](https://github.com/CppCXY/emmylua-analyzer-rust/releases)
[![Rust](https://img.shields.io/badge/built_with-Rust-orange?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![Crates.io](https://img.shields.io/crates/d/emmylua_ls?style=for-the-badge&logo=rust&color=orange)](https://crates.io/crates/emmylua_ls)

</div>

<div align="center">

### 🔗 Quick Navigation

[🚀 **Quick Start**](#-quick-start) • [✨ **Features**](#-features) • [📦 **Installation**](#-installation) • [📖 **Documentation**](#-documentation) • [🛠️ **Development**](#-development)

</div>

---

<div align="center">

## 💫 Revolutionary Lua Development Experience

*Powered by Rust's blazing performance and memory safety*

</div>

### 🎯 What Makes Us Different

<table>
<tr>
<td width="50%">

#### ⚡ **Performance First**
- **10x faster** than traditional Lua Language servers
- **Zero-cost abstractions** with Rust
- **Incremental compilation** for instant feedback
- **Memory-efficient** analysis engine

</td>
<td width="50%">

#### 🧠 **Intelligent Analysis**
- **Advanced type inference** system
- **Cross-reference resolution**
- **Semantic highlighting** with context
- **Real-time error detection**

</td>
</tr>
<tr>
<td width="50%">

#### 🔧 **Universal Compatibility**
- **Lua 5.1** through **5.5** support
- **LuaJIT** optimization
- **Cross-platform** deployment
- **Editor-agnostic** LSP implementation

</td>
<td width="50%">

#### 📚 **Developer Ecosystem**
- **Rich documentation** generation
- **Code formatting** and style enforcement
- **Static analysis** and linting
- **Project scaffolding** tools

</td>
</tr>
</table>

---

## ✨ Features

<div align="center">

### 🎯 Core Capabilities

</div>

<table>
<tr>
<td width="50%">

#### 🔍 **Language Support**
- ✅ **Lua 5.1** - Full compatibility
- ✅ **Lua 5.2** - Complete feature set
- ✅ **Lua 5.3** - Integer types & UTF-8
- ✅ **Lua 5.4** - Attributes & generational GC
- ✅ **Lua 5.5** - New global syntax
- ✅ **LuaJIT** - Performance optimizations

</td>
<td width="50%">

#### 📝 **Annotation System**
- ✅ **EmmyLua** annotations
- ✅ **Luacats** documentation

</td>
</tr>
<tr>
<td width="50%">

#### 🛠️ **LSP Features**
- ✅ **Auto-completion** with context
- ✅ **Go to definition**
- ✅ **Find references**
- ✅ **Go to implementation**
- ✅ **Hover information**
- ✅ **Signature help**
- ✅ **Rename refactoring**
- ✅ **Code actions**
- ✅ **Diagnostics**
- ✅ **Document symbols**
- ✅ **Workspace symbols**
- ✅ **Code formatting**
- ✅ **Code folding**
- ✅ **Document links**
- ✅ **Semantic tokens**
- ✅ **Inlay hints**
- ✅ **Document highlights**
- ✅ **Code lens**
- ✅ **Call hierarchy**
- ✅ **Symbol search**
- ✅ **Document color**


</td>
<td width="50%">

#### 🎨 **Code Quality**
- ✅ **Syntax highlighting**
- ✅ **Error detection**
- ✅ **Code formatting**
- ✅ **Style enforcement**
- ✅ **More Linting**

</td>
</tr>
</table>

---

## 🚀 Quick Start

### Prerequisites

Before getting started, ensure you have Rust installed on your system:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 📦 Installation

Choose your preferred installation method:

<details>
<summary><b>🦀 Via Cargo</b></summary>

```bash
# Install the language server
cargo install emmylua_ls

# Install documentation generator
cargo install emmylua_doc_cli

# Install static analyzer
cargo install emmylua_check
```

</details>

<details>
<summary><b>📥 Pre-built Binaries</b></summary>

Download the latest binaries from our [releases page](https://github.com/CppCXY/emmylua-analyzer-rust/releases).

</details>

<details>
<summary><b>🔧 Build from Source</b></summary>

```bash
git clone https://github.com/EmmyLuaLs/emmylua-analyzer-rust.git
cd emmylua-analyzer-rust
cargo build --release -p emmylua_ls
```

</details>

### 🎮 Editor Integration

<details>
<summary><b>VS Code</b></summary>

Install the [EmmyLua Extension](https://marketplace.visualstudio.com/items?itemName=tangzx.emmylua) for the best development experience.

</details>

<details>
<summary><b>Neovim</b></summary>

Configure with your LSP client:

```lua
vim.lsp.enable({"emmylua_ls"})
```

</details>
<details>
<summary><b>Intellij IDE</b></summary>

Install the [EmmyLua2 Plugin](https://plugins.jetbrains.com/plugin/25076-emmylua2) from the JetBrains Marketplace.

</details>

<details>
<summary><b>Other Editors</b></summary>

EmmyLua Analyzer Rust implements the standard LSP protocol, making it compatible with any editor that supports LSP.

</details>

---

## 📖 Documentation

- [📖 **Features Guide**](./docs/features/features_EN.md) - Comprehensive feature documentation
- [⚙️ **Configuration**](./docs/config/emmyrc_json_EN.md) - Advanced configuration options
- [🧾 **Formatter Guide**](./docs/emmylua_formatter/README_EN.md) - Formatter behavior, options, and usage guide
- [🖼️ **Formatter Examples**](./docs/emmylua_formatter/examples_EN.md) - Before-and-after formatting examples
- [🛠️ **Formatter Options**](./docs/emmylua_formatter/options_EN.md) - Formatter configuration reference
- [📐 **Formatter Profiles**](./docs/emmylua_formatter/profiles_EN.md) - Recommended formatter configurations for common styles
- [📚 **Formatter Tutorial**](./docs/emmylua_formatter/tutorial_EN.md) - Practical formatting workflow and examples
- [📝 **Annotations Reference**](./docs/emmylua_doc/annotations_EN/README.md) - Detailed annotation documentation
- [🎨 **Code Style**](https://github.com/CppCXY/EmmyLuaCodeStyle/blob/master/README_EN.md) - Formatting and style guidelines
- [🛠️ **External Formatter Integration**](./docs/external_format/external_formatter_options_EN.md) - Using external formatters
---

## 🛠️ Usage & Examples

### 🖥️ Language Server (`emmylua_ls`)

Start the language server with default settings:

```bash
emmylua_ls
```

Advanced usage with custom configuration:

```bash
# TCP mode for remote debugging
emmylua_ls -c tcp --port 5007 --log-level debug --log-path ./logs

# Stdio mode (default)
emmylua_ls -c stdio --log-level info

# Stdio mode default parameters
emmylua_ls
```

**Server Parameters:**
- `-c, --communication`: Communication method (`stdio` | `tcp`)
- `--port`: TCP port when using TCP mode (default: 5007)
- `--log-level`: Logging level (`debug` | `info` | `warn` | `error`)
- `--log-path`: Directory for log files

### 📚 Documentation Generator (`emmylua_doc_cli`)

Generate beautiful API documentation:

```bash
# Basic usage
emmylua_doc_cli ./src --output ./docs
```

### ✅ Static Analyzer (`emmylua_check`)

Perform comprehensive code analysis:

```bash
# Analyze current workspace
emmylua_check .

# Analyze specific directory with detailed output
emmylua_check ./src --verbose --format json
```

---

## 🏗️ Development

### Building from Source

```bash
# Clone the repository
git clone https://github.com/EmmyLuaLs/emmylua-analyzer-rust.git
cd emmylua-analyzer-rust

# Build all crates
cargo build --release

# Build specific components
cargo build --release -p emmylua_ls
cargo build --release -p emmylua_doc_cli
cargo build --release -p emmylua_check
```

### Testing

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p emmylua_parser

# Run with coverage
cargo test --all-features --no-fail-fast
```

### Contributing

We welcome contributions!.

More details can be found in our [CONTRIBUTING.md](./CONTRIBUTING.md) file.

---

## 📄 License

This project is licensed under the [MIT License](./LICENSE) - see the LICENSE file for details.

---

<div align="center">

### 🙏 Acknowledgments

Special thanks to all contributors and the Lua community for their continuous support.

[⬆ Back to Top](#-emmylua-analyzer-rust)

</div>

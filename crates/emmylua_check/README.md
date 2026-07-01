<div align="center">

# 🦀 EmmyLua Check

[![Crates.io](https://img.shields.io/crates/v/emmylua_check.svg?style=for-the-badge&logo=rust)](https://crates.io/crates/emmylua_check)
[![GitHub license](https://img.shields.io/github/license/CppCXY/emmylua-analyzer-rust?style=for-the-badge&logo=mit&color=blue)](../../LICENSE)

</div>

`emmylua_check` is a powerful command-line tool designed to help developers identify and fix potential issues in Lua code during development. It leverages the core analysis engine of `emmylua-analyzer` to provide comprehensive code diagnostics, ensuring code quality and robustness.

---

## ✨ Features

- **⚡ High Performance**: Built with Rust for blazing-fast analysis, capable of handling large codebases.
- **🎯 Comprehensive Diagnostics**: Offers over 50 types of diagnostics, including:
  - Syntax errors
  - Type mismatches
  - Undefined variables and fields
  - Unused code
  - Code style issues
  - ...and more!
- **⚙️ Highly Configurable**: Fine-grained control over each diagnostic via `.emmyrc.json` or `.luarc.json` files, including enabling/disabling and severity levels.
- **💻 Cross-Platform**: Supports Windows, macOS, and Linux.
- **CI/CD Friendly**: Easily integrates into continuous integration workflows to ensure team code quality.

---

## 📦 Installation

Install `emmylua_check` via cargo:
```shell
cargo install emmylua_check
```

---

## 🚀 Usage

### Basic Usage

Analyze all Lua files in the current directory:
```shell
emmylua_check .
```

Analyze a specific workspace directory:
```shell
emmylua_check ./src
```

### Advanced Usage

#### Specify Configuration File

Use a specific `.emmyrc.json` configuration file:
```shell
emmylua_check . -c ./config/.emmyrc.json
```

#### Ignore Specific Files or Directories

Ignore files in the `vender` and `test` directories:
```shell
emmylua_check . -i "vender/**,test/**"
```

#### Output in JSON Format

Output diagnostics in JSON format to a file for further processing:
```shell
emmylua_check . -f json --output ./diag.json
```

#### Filter by Severity

Only output warnings and errors:
```shell
emmylua_check . --severity warn
```

---

## ⚙️ Configuration

`emmylua_check` shares the same configuration system as the EmmyLua Language Server. You can create a `.emmyrc.json` file in your project root to configure diagnostic rules.

**Example `.emmyrc.json`:**
```json
{
  "diagnostics": {
    "disable": [
      "unused"
    ]
  }
}
```

For detailed information on all available diagnostics and configuration options, see the [**EmmyLua Configuration Documentation**](../../docs/config/emmyrc_json_CN.md).

---

## 🛠️ CI/CD Integration

You can easily integrate `emmylua_check` into your GitHub Actions workflow to automate code checks.

**Example `.github/workflows/check.yml`:**
```yaml
name: EmmyLua Check

on: [push, pull_request]

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
      - name: Install emmylua_check
        run: cargo install emmylua_check
      - name: Run check
        run: emmylua_check .
```

---

## Command Line Options

```
Usage: emmylua_check [OPTIONS] [WORKSPACE]...

Arguments:
  [WORKSPACE]...  Path(s) to workspace directory

Options:
  -c, --config <CONFIG>                Path to configuration file. If not provided, ".emmyrc.json" and ".luarc.json" will be searched in the workspace directory
  -i, --ignore <IGNORE>                Comma-separated list of ignore patterns. Patterns must follow glob syntax
  -f, --output-format <OUTPUT_FORMAT>  Specify output format [default: text] [possible values: json, text, sarif]
      --output <OUTPUT>                Specify output target (stdout or file path, only used when output_format is json) [default: stdout]
      --warnings-as-errors             Treat warnings as errors
      --severity <SEVERITY>            Only output diagnostics at this severity or above [possible values: error, warn, info, hint]
      --verbose                        Verbose output
  -h, --help                           Print help information
  -V, --version                        Print version information
```

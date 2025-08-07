<div align="center">

# 📚 EmmyLua Doc CLI

[![Crates.io](https://img.shields.io/crates/v/emmylua_doc_cli.svg?style=for-the-badge&logo=rust)](https://crates.io/crates/emmylua_doc_cli)
[![GitHub license](https://img.shields.io/github/license/CppCXY/emmylua-analyzer-rust?style=for-the-badge&logo=mit&color=blue)](../../LICENSE)

</div>

`emmylua_doc_cli` is a powerful command-line tool for generating documentation directly from your Lua source code and EmmyLua annotations. Built with Rust, it offers exceptional performance and is a core component of the `emmylua-analyzer-rust` ecosystem.

---

## ✨ Features

- **🚀 Blazing Fast**: Leverages Rust's performance to parse and generate documentation for large codebases in seconds.
- **✍️ Rich Annotation Support**: Intelligently interprets EmmyLua annotations (`---@class`, `---@field`, `---@param`, etc.) to generate detailed and accurate documentation.
- **🔧 Highly Customizable**:
    - Override the default templates with `--override-template` to match your project's branding.
    - Inject custom content into the main page using the `--mixin` option to add guides, tutorials, or other static pages.
- **📦 Multiple Output Formats**: Generate documentation in **Markdown** or **JSON** for maximum flexibility.
- **🤝 CI/CD Ready**: Automate your documentation publishing workflow with seamless integration into services like GitHub Actions.

---

## 📦 Installation

Install `emmylua_doc_cli` via cargo:
```shell
cargo install emmylua_doc_cli
```
Alternatively, you can grab pre-built binaries from the [**GitHub Releases**](https://github.com/EmmyLua/emmylua-analyzer-rust/releases) page.

---

## 🚀 Usage

### Basic Usage

Generate documentation for all Lua files in the `src` directory and output to the default `./docs` folder:
```shell
emmylua_doc_cli ./src -o ./docs
```

### Advanced Usage

#### Generate JSON Output

Output the documentation structure as a JSON file for custom processing:
```shell
emmylua_doc_cli . -f json -o ./api.json
```

#### Customize Site Name

Set a custom name for the generated documentation site:
```shell
emmylua_doc_cli . -o ./docs --site-name "My Awesome Project"
```

#### Ignore Files

Exclude certain directories or files from the documentation:
```shell
emmylua_doc_cli . -o ./docs --ignore "third_party/**,test/**"
```

---

## 🛠️ CI/CD Integration

Automate the process of building and deploying your documentation to GitHub Pages using GitHub Actions.

**Example `.github/workflows/docs.yml`:**
```yaml
name: Generate and Deploy Docs

on:
  push:
    branches:
      - main

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Install emmylua_doc_cli
        run: cargo install emmylua_doc_cli

      - name: Generate Docs
        run: emmylua_doc_cli ./src -o ./docs --site-name "My Project"
```

---

## Command Line Options

```
Usage: emmylua_doc_cli [OPTIONS] [WORKSPACE]...

Arguments:
  [WORKSPACE]...  Path to the workspace directory

Options:
  -c, --config <CONFIG>                        Configuration file paths. If not provided, both ".emmyrc.json" and ".luarc.json" will be searched in the workspace directory
      --ignore <IGNORE>                        Comma separated list of ignore patterns. Patterns must follow glob syntax
  -f, --output-format <OUTPUT_FORMAT>          Specify output format [default: markdown] [possible values: json, markdown]
  -o, --output <OUTPUT>                        Specify output destination (can be stdout when output_format is json) [default: ./output]
      --override-template <OVERRIDE_TEMPLATE>  The path of the override template
      --site-name <SITE_NAME>                  [default: Docs]
      --mixin <MIXIN>                          The path of the mixin md file
      --verbose                                Verbose output
  -h, --help                                   Print help
  -V, --version                                Print version
```

# Contributing

## Crates Overview

Our project is organized into several crates:

| Crate                                                          | Badge                                                                                                                                                   | Description |
|----------------------------------------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------| ----------- |
| [🔍 **emmylua_parser**](./crates/emmylua_parser)               | [![emmylua_parser](https://img.shields.io/crates/v/emmylua_parser.svg?style=flat-square)](https://crates.io/crates/emmylua_parser)                      | The foundational Rust-based Lua parser engineered for maximum efficiency and accuracy. Powers all downstream analysis tools. |
| [📑 **emmylua_parser_desc**](./crates/emmylua_parser_desc)     | [![emmylua_parser_desc](https://img.shields.io/crates/v/emmylua_parser_desc.svg?style=flat-square)](https://crates.io/crates/emmylua_parser_desc)       | Extension for EmmyLua-Parser that handles Markdown/RST highlighting in comments. |
| [🧠 **emmylua_code_analysis**](./crates/emmylua_code_analysis) | [![emmylua_code_analysis](https://img.shields.io/crates/v/emmylua_code_analysis.svg?style=flat-square)](https://crates.io/crates/emmylua_code_analysis) | Advanced semantic analysis engine providing deep code understanding, type inference, and cross-reference resolution. |
| [🖥️ **emmylua_ls**](./crates/emmylua_ls)                       | [![emmylua_ls](https://img.shields.io/crates/v/emmylua_ls.svg?style=flat-square)](https://crates.io/crates/emmylua_ls)                                  | The complete Language Server Protocol implementation offering rich IDE features across all major editors. |
| [📚 **emmylua_doc_cli**](./crates/emmylua_doc_cli/)            | [![emmylua_doc_cli](https://img.shields.io/crates/v/emmylua_doc_cli.svg?style=flat-square)](https://crates.io/crates/emmylua_doc_cli)                   | Professional documentation generator creating beautiful, searchable API docs from your Lua code and annotations. |
| [✅ **emmylua_check**](./crates/emmylua_check)                 | [![emmylua_check](https://img.shields.io/crates/v/emmylua_check.svg?style=flat-square)](https://crates.io/crates/emmylua_check)                         | Comprehensive static analysis tool for code quality assurance, catching bugs before they reach production. |
| [🧹 **emmylua_formatter**](./crates/emmylua_formatter)         | [![emmylua_formatter](https://img.shields.io/crates/v/emmylua_formatter.svg?style=flat-square)](https://crates.io/crates/emmylua_formatter)             | The Lua formatter used by the EmmyLua Analyzer Rust workspace. |


## Testing

We use the standard Rust testing harness, along with assert macros from [`googletest-rust`]:

```shell
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p emmylua_parser
```

If you're unfamiliar with `googletest-rust`, here's a quick overview:

- Use `googletest::prelude::*` in your test modules, and annotate test functions with `#[gtest]`.

- `assert_that!` checks a condition and panics on error:

  ```rust
  assert_that!(2 * 2, eq(4));
  ```

  Prefer `assert_that!(x, eq(y))` to `assert_eq` because the former generates a nice diff for you.

- `expect_that!` checks a condition, marks test as failed on error, and continues execution.

  This is useful when adding multiple test cases to a single `#[gtest]` function:

  ```rust
  // Both expectations will be evaluated and reported when test fails:
  expect_that!(2 * 2, ne(4));
  expect_that!(3 * 3, ne(9));
  ```

- `verify_that!` checks a condition and returns a `googletest::Result`.

- `OrFail::or_fail` converts any `Optional` and `Result` to a `googletest::Result`. It also adds current location
  to an error message. We have a wrapper around it called [`check!`].


## Code style and formatting

We use [`rustfmt`] and [`pre-commit`] to manage project's code style.

- `rustfmt` formats Rust code. Simply run `cargo fmt --all` to reformat all files.

- `pre-commit` fixes common issues like trailing whitespaces or broken symlinks in all text files.

  To run it,

  1. install [`pre-commit`][pre-commit-install],
  2. invoke `pre-commit run --all`.

  If it suits your workflow, you can configure PreCommit to run before every commit. To do so, run `pre-commit install`.
  Note that this is not required because our CI will detect any issues.

[`googletest-rust`]: https://github.com/google/googletest-rust/
[`rustfmt`]: https://rust-lang.github.io/rustfmt/
[`pre-commit`]: https://pre-commit.com/#install
[pre-commit-install]: https://pre-commit.com/#install
[`check!`]: https://github.com/search?q=repo%3AEmmyLuaLs%2Femmylua-analyzer-rust%20macro_rules!%20check&type=code

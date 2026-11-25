# crate path lint

`lint/crate_paths.py` keeps references short: code uses `module::Item`, not `crate::module::Item`.

## Check (CI)

```bash
python lint/crate_paths.py check --root crates
```

Exit code 1 when any non-`use` `crate::...` occurrence remains, printing file:line.

## Fix

```bash
python lint/crate_paths.py fix --root crates/cpp_parser
```

The fix imports the first segment once per file (or inline module) and rewrites only code, never `use` declarations. After writing it runs `cargo check --workspace --all-targets`; if that fails, every changed file is restored and the command exits non-zero. `--no-verify` skips that guard for a fast local experiment.

The fixer is conservative: a short name that is already imported in a conflicting way is skipped and reported, rather than forcing an ambiguous import. Run it per crate/module, not over the whole tree at once, and keep the resulting diff reviewable.


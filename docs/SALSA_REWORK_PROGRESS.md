# Salsa Rework Progress & Handoff Notes

> This document is intended for the next AI/developer continuing this work.
> It records the current architecture, what has already been done, what is still slow,
> and the exact next steps we agreed on.

## 1. Current Architecture

We have **abandoned high-level per-node Salsa-ification**.

The current design is:

```text
SourceFileInput
   ↓
salsa:
  parse / line_index / document / file_facts / flow_tree / file_exports
  ↓
per-workspace indexes:
  workspace_decl_index_for
  workspace_type_index_for
  workspace_member_index_for
  workspace_reference_index_for
  workspace_module_index_for
  ↓
SemanticModel (short-lived, per file):
  immutable db + local cache
  high-frequency semantic queries run here:
    type_of_expr / type_of_decl / type_of_member
    resolve_member / member_info / member_infos / type_check
    type_of_expr_at / type_of_member_at / type_of_decl_at
    callable_candidates / call_site_analysis
  SemanticModel is discarded after use
```

### Key principle

- Salsa is only used for **stable, coarse-grained, per-file / per-workspace** data.
- High-frequency, fine-grained semantic queries **must not** become Salsa queries.
- `SemanticModel` owns a local `SemanticLocalCache` and is thrown away after each file analysis.

## 2. What Has Been Done

### 2.1 Removed high-level Salsa queries

Deleted all `semantic_*` Salsa queries from `salsa_builder/query.rs`:

- `semantic_expr_type`
- `semantic_type_check`
- `semantic_resolve_member`
- `semantic_decl_type`
- `semantic_member_type`
- `semantic_member_infos`
- `semantic_member_info`
- `semantic_expr_type_at`
- `semantic_member_type_at`
- `semantic_decl_flow_type`
- `semantic_callable_candidates`
- `semantic_call_site`

Also removed Salsa mirror types:

- `SalsaResolvedMember`
- `SalsaMemberKey`
- `SalsaMemberInfo`
- `SalsaCallSiteAnalysis`

### 2.2 Restored SemanticModel local cache

File: `crates/emmylua_code_analysis/src/semantic_model/cache.rs`

```rust
pub(crate) struct SemanticLocalCache {
    expr_type: HashMap<...>,
    decl_type: HashMap<...>,
    member_type: HashMap<...>,
    resolve_member: HashMap<...>,
    expr_type_at: HashMap<...>,
    member_type_at: HashMap<...>,
    flow_decl: HashMap<...>,
    callable_candidates: HashMap<...>,
    call_site: HashMap<...>,
    member_infos: HashMap<...>,
    member_info: HashMap<...>,
    type_check: HashMap<...>,
}
```

This cache is a field of `SemanticModel` and is dropped when the model is dropped.

### 2.3 All workspace indexes are now per-workspace

Removed monolithic indexes:

- `workspace_decl_index`
- `workspace_type_index`
- `workspace_member_index`
- `workspace_reference_index`
- `workspace_module_index`

Replaced with:

- `workspace_decl_index_for`
- `workspace_type_index_for`
- `workspace_member_index_for`
- `workspace_reference_index_for`
- `workspace_module_index_for`

This means editing `main` workspace no longer rebuilds `std` / library indexes.

`ModuleNodeId` now includes `workspace_id` to avoid node-id collisions across workspaces.

### 2.4 Name resolution optimized

Local declarations:

- `FileFacts` now builds `visible_decls_by_name` during fact construction.
- `find_visible_decl_before_offset` uses HashMap + binary search + short reverse scan.

Global declarations:

- `WorkspaceDeclIndex` now has `global_by_name: HashMap<SmolStr, SemanticId>`.
- `global_decl_named` is O(1) HashMap lookup.

DeprecatedChecker:

- Added `deprecated_global_names_for` per workspace.
- `DeprecatedChecker` now checks a `HashSet<SmolStr>` instead of resolving every global name through `global_decl_by_name`.

## 3. Performance Data (real project)

Workspace used for testing:

```text
C:\Users\zc\Desktop\N5_project\ProjectN5_Server\gameserver\script
```

- 1452 Lua files
- about 8.9 MB source

### Before these fixes

- Memory: kept growing without bound.
- Single small file after warm: ~49ms.
- `DeprecatedChecker`: ~19ms on a 2.5KB file.

### After these fixes

- Memory: no longer explodes.
- Single 2.5KB file after warm (deprecated set already built): ~28.7ms.
- `DeprecatedChecker`: ~2ms after deprecated set is warm.
- One-time cost to build per-workspace deprecated global set still exists.

## 4. Remaining Bottlenecks

Per-file diagnosis after warm is still too slow for 1452 files.

Current hot checkers on a 2.5KB file:

| checker | approx time |
|---|---|
| `AssignTypeMismatchChecker` | ~5.6ms |
| `CheckExportChecker` | ~1.7ms |
| `NeedCheckNilChecker` | ~1.1ms |
| `AccessInvisibleChecker` | ~1ms |
| `DeprecatedChecker` | ~2ms (warm) |

Full serial workspace estimate:

```text
1452 files * ~30ms ≈ 43s
```

This is still too slow.

## 5. Next Steps (for the next AI)

### 5.1 Optimize `AssignTypeMismatchChecker`

This is now the biggest per-file checker.

- Profile its internal calls:
  - `type_of_expr`
  - `member_info`
  - `type_check`
- Add more aggressive short-circuiting in `type_check`:
  - identical types
  - primitive vs primitive
  - simple non-union / non-intersection cases before entering full unify
- Make sure `SemanticModel` local cache is actually hit for repeated `type_check` pairs.

### 5.2 Optimize `CheckExportChecker`

- It is currently ~1.7ms even on a small file.
- Check whether it re-scans the same export surface repeatedly.
- Precompute file-level export surface once in `SemanticModel` local cache.

### 5.3 Optimize `NeedCheckNilChecker`

- ~1.1ms on a small file.
- Look for repeated `type_of_expr` calls on the same expressions.
- Consider batching or caching flow-sensitive nil checks per body.

### 5.4 Optimize `AccessInvisibleChecker`

- ~1ms on a small file.
- Check whether it repeatedly resolves the same declaration / member.

### 5.5 Consider parallel full-workspace diagnostics

- `pull_workspace_diagnostics_slow` is intentionally serial for CPU/cancellation reasons.
- If per-file cost drops enough, serial may be acceptable.
- Do not blindly parallelize slow path; it was intentionally kept serial.

### 5.6 Do not re-introduce high-level Salsa queries

Important warning for future work:

- Do **not** put `type_of_expr`, `member_info`, `call_site`, `type_check` back into Salsa.
- Do **not** use `ArcIntern<LuaType>` or full `LuaType` as Salsa keys/values.
- Keep Salsa limited to coarse per-file / per-workspace data.
- Keep high-frequency semantic caches inside `SemanticModel`.

## 6. Useful Commands

All commands should be run with a timeout to avoid hanging the machine.

```bash
# Check
timeout 120s cargo check --workspace

# Core tests
timeout 120s cargo test -p emmylua_code_analysis --lib

# LS tests
timeout 120s cargo test -p emmylua_ls --lib

# Single-file benchmark (loads full workspace, diagnoses one file)
timeout 120s cargo run -q -p emmylua_check --bin bench_file -- \
  "<workspace-root>" "<target-file>"
```

## 7. Key Files

- `crates/emmylua_code_analysis/src/semantic_model/cache.rs` — local cache
- `crates/emmylua_code_analysis/src/semantic_model/mod.rs` — SemanticModel methods
- `crates/emmylua_code_analysis/src/salsa_builder/query.rs` — per-workspace indexes
- `crates/emmylua_code_analysis/src/salsa_builder/facts.rs` — FileFacts + name lookup indexes
- `crates/emmylua_code_analysis/src/check/checker/deprecated.rs` — deprecated checker

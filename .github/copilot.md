# Salsa Migration Plan

## Goal

- `DbIndex` remains the compilation container, but its source of truth becomes the salsa summary database.
- `summary_builder.analysis` is restricted to single-file syntax-derived facts.
- `summary_builder.query` becomes the derived query layer on top of those facts.
- `compilation` becomes the projection/bridge layer for existing consumers.
- `compilation::analyzer` is deprecated and will be removed instead of being evolved further.

## Architecture Rules

- `summary_builder.analysis` may read parser AST only.
- `summary_builder.analysis` may not read `DbIndex`.
- `summary_builder.analysis` may not perform cross-file resolution.
- `summary_builder.analysis` may not write legacy indexes.
- `summary_builder.query` may aggregate facts and cross-file metadata, but may not mutate compilation state.
- `LuaCompilation::update_index()` should mean "sync facts", not "run analyzer pipeline".

## Migration Stages

### 1. Mount salsa into `DbIndex`

- Add `SalsaSummaryDatabase` as a field on `DbIndex`.
- Treat `Vfs`, config, and workspace roots stored in `DbIndex` as the only production inputs.
- Sync config, file snapshots, file removals, and clears into the salsa database through `DbIndex` methods.
- Keep `SalsaSummaryHost` as a test helper only; production code should no longer need a second VFS-owning host.

### 2. Change `LuaCompilation::update_index()` to sync facts only

- Stop building `AnalyzeContext` and stop calling `compilation::analyzer::analyze(...)` from `update_index()`.
- `update_index()` should only:
  - sync workspace metadata into salsa
  - sync the changed file snapshots into salsa
- Legacy analyzer-backed indexes will temporarily go stale; this is expected during the migration.

### 3. Introduce compilation projections

- Build thin summary-first projection modules in `compilation`, for example:
  - `compilation::module`
  - `compilation::types`
  - `compilation::doc`
  - `compilation::flow`
  - `compilation::semantic_target`
- These projections adapt summary/query results to the old APIs without reintroducing analyzer-style mutation.

### 4. Replace analyzer outputs incrementally

- Migrate read paths in this order:
  1. decl/doc/module/property
  2. lexical/flow
  3. signature return / semantic graph / solver
  4. type query / program point / narrowing
  5. diagnostic inputs
  6. direct semantic model queries

### 5. Remove analyzer

- Once `update_index()` and the main read paths no longer depend on analyzer-written indexes, delete:
  - analyzer pipeline entrypoints
  - analyzer-only write paths into `DbIndex`
  - dead compatibility caches

## What counts as facts

- decl tree
- doc summaries and doc type nodes
- flow summaries
- lexical uses
- property summaries
- table shape summaries
- module export syntax facts
- semantic graph and solver inputs that are deterministic derivations of file facts

## What should not remain as core facts

- mutable diagnostic accumulation
- compatibility maps that only exist for old APIs
- analyzer-phase temporary state
- duplicated caches whose only purpose is to shadow summary/query results

## Immediate implementation focus

- Step 1: embed `SalsaSummaryDatabase` into `DbIndex`.
- Step 2: make `LuaCompilation::update_index()` sync salsa facts only.
- Validation target: `cargo check -p emmylua_code_analysis`.

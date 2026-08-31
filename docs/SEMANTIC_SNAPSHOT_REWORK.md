# Semantic Snapshot Rework

## Problem

The current salsa layer caches per-file facts and TypeShell-level queries, but the
high-level semantic layer is still rebuilt on every LSP/diagnostic request.

`SemanticModel` is a short-lived view containing instance-local `RefCell` caches.
Because `LuaType` is a recursive tree with pointer-identity hashing, these caches
were deliberately not moved into salsa, so they cannot be reused across requests,
threads, or salsa snapshots.

As a result:

- expression inference (`type_of_expr`) recompiles and re-runs the VM on every call;
- `member_infos` / `member_info` re-traverse type hierarchies on every call;
- `resolve_member` and call-site analysis are recomputed for every feature;
- every LSP handler creates a fresh `SemanticModel`, losing all warm caches.

## Target Architecture

```text
SourceFileInput
      |
      v
parse / line_index / file_facts          (per-file, already salsa-tracked)
      |
      v
FileSemantic / FileSemanticSnapshot      (per-file immutable snapshot)
      |
      v
High-level semantic queries                (salsa-tracked)
  - lua_expr_type
  - member_infos_for_type
  - resolve_member
  - call_site_candidates
  - type_check
      |
      v
SemanticModel as a thin query facade
```

## Implementation Phases

### Phase 1: Shared high-level semantic cache bridge

Goal: stop losing all high-level caches when a new `SemanticModel` is created, while
avoiding a risky full salsa-query conversion before `LuaType` interning lands.

Current implementation:

- `SemanticCache` is stored as `Arc<SemanticCache>` inside `SalsaDatabase`.
- Cloned `SalsaDatabase` snapshots share the same cache.
- Stable keys are used:
  - `(file_id, LuaSyntaxId)` for expression types, `resolve_member`, callable candidates and call-site analysis;
  - `(file_id, SemanticId)` for declaration/member types;
  - `(file_id, LuaSyntaxId, TextSize)` / `(file_id, SemanticId, TextSize)` for flow-sensitive types.
- Member info lists, member-info-by-key, callable candidates, call-site analysis,
  type compatibility and flow-decl backtracking results are also shared in this
  bridge cache. Several are still keyed by structural `LuaType` for now; this is
  acceptable because the bridge cache is cleared on every mutation, unlike a
  long-lived salsa memo.
- The cache is cleared on any file/config/workspace mutation.
- The temporary `CSEMPTY` / `CS` profiling prints in call-site analysis were removed.
- `SemanticModel` no longer owns the high-frequency semantic caches; it only retains
  recursion guards and per-pass state.
- Later phases replace this bridge with actual salsa-tracked semantic queries once
  type interning makes `LuaType` safe to use as a query key.

Expected benefit:

- repeated LSP features on an unchanged workspace share warm high-level results;
- `salsa_snapshot()` clones start providing cross-thread reuse for these caches;
- no change to public API shape at first.

### Phase 2: Type arena / interning

Current implementation:

- `SemanticCache` now uses `internment::ArcIntern<LuaType>` as the interned
  handle for structural-type cache keys.
- `member_infos`, `member_info` and `type_check` caches store keys as
  `ArcIntern<LuaType>` instead of full recursive `LuaType` values.
- This gives the shared cache a cheap, stable, equality-based interned key while
  keeping the public `LuaType` API unchanged.

Next sub-steps before considering Phase 3 complete:

- Expose a public/internal `InternedLuaType` alias if other query layers need it.
- Use `ArcIntern<LuaType>` as a salsa query key once the high-level semantic
  queries are moved into salsa memo.
- Add an explicit arena/storage type if the global interner proves insufficient.

### Phase 3: Workspace index flattening

Current implementation:

- `WorkspaceMemberIndex` now stores `Arc<[MemberRef]>` per owner.
- `members_of_owner` exposes an `Arc`-backed `MemberList` wrapper.
- `WorkspaceTypeIndex`: `type_defs_in_scope` and `resolve_type_def_locations` now
  return `Arc<[TypeDef]>`; the public facade exposes an `Arc`-backed `TypeDefList`.
- Public callers can still iterate with the previous owned-item API shape via the
  wrapper's `IntoIterator`.

P3 follow-up:

- `WorkspaceTypeIndex` now precomputes per-bucket `Arc<[TypeDef]>`; `find_all`
  returns a shared slice without cloning matching `TypeDef` values on every call.
- The remaining opportunity is applying the same `Arc` treatment to the remaining
  hot workspace indexes where needed.

### Phase 4: Body-level inference

Current implementation:

- `SemanticCache` now holds per-body inference maps:
  `HashMap<(FileId, body_syntax_id), Arc<RwLock<HashMap<LuaSyntaxId, LuaType>>>>`.
- `type_of_expr` lazily records each computed expression type under its enclosing
  closure/function body (or the chunk at file scope).
- Repeated expression queries in the same logical body hit the shared body map.

Note: the first eager batch implementation inferred all body expressions up front,
but it broke flow/call-site-dependent tests because some expressions are only correct
when queried after their flow/context is available. The current implementation is
therefore lazy per-expression but grouped per body, preserving correctness.

Remaining work to reach rust-analyzer-style body inference:

- Replace this bridge with a real `BodyInference` salsa query after expression
  inference is made fully flow/context-safe.
- Consider batch inference with a deterministic dependency order once the VM can
  safely resolve all body expressions without premature Unknown results.

### Phase 5: Parallel snapshot reuse

Current implementation:

- `SalsaDatabase::parallel_for_each_file` runs a `Sync` callback for every workspace
  file on scoped worker threads.
- Each worker owns its own `SalsaDatabase` clone, sharing:
  - salsa memo storage;
  - `SemanticCache` (Arc-owned in the database snapshot);
  - file facts / type indexes.
- A test covers concurrent per-file salsa queries from worker-owned snapshots.

Migrated handlers:

- `workspace_symbols`
- `emmy_gutter` detail lookup
- `implementation` searcher (member discovery and same-name global discovery)

Remaining opportunity:

- Further workspace-wide LS handlers can still be migrated where ordering is not
  required.
- Once high-level queries are truly salsa-tracked, parallelism will also share
  memoized inference results rather than only the bridge cache.

## Non-goals for now

- Do not introduce `LuaType` as a salsa key directly.
- Do not rewrite the VM or type-check algorithm yet.
- Do not remove the existing `SemanticModel` API.

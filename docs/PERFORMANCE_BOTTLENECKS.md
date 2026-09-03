# Performance Bottlenecks & Profiling Notes

> This document records profiling findings for the Salsa-based semantic/check pipeline.
> It is intended for future performance reviews and regression tracking.

## How to profile

### Whole workspace checker summary

```bash
cargo run -p emmylua_check --bin emmylua_check -- --profile <workspace> -f text
```

Prints a per-checker total/average table after the run.

### Single file / focused benchmark

```bash
cargo build --release -p emmylua_check --bin bench_file
target/release/bench_file.exe <workspace> <target-lua-file>
```

Prints `TOTAL` time for `diagnose_file` on the target file.

### Temporary internal instrumentation

The following env variables were used during exploration and are not part of the committed code:

- `PARAM_PROFILE=1`
- `CALL_SITE_PROFILE=1`
- `MEMBER_PROFILE=1`

They can be reintroduced temporarily when a deeper internal profile is needed.

## Current major hotspot: `call_site_analysis`

Measured on a real project file:

- `player/player.lua` (~2495 lines)
- Release build, warm process
- `ParamTypeChecker` total: ~500-560ms
- `call_site_analysis`: ~540ms of that

Breakdown:

| Category | Calls | Time |
|---|---:|---:|
| Empty candidates (unresolved member calls) | 282 | ~277ms |
| Non-empty candidates: candidate resolution | 297 | ~19ms |
| Non-empty candidates: argument `type_of_expr` | 297 | ~240ms |
| Receiver type inference | — | ~3ms |

### Empty-candidate path is the first thing to fix

`member_callable_candidates` accounts for almost all empty-candidate time:

- 281 empty member callable resolutions: ~280ms
- Inside those:
  - `resolve_member`: ~134ms
  - member_id == None fallback branch: ~128ms
- One pathological call (`pPlayer.Msg`) alone costs ~83ms inside the fallback branch.

This suggests:

1. Many `pPlayer.*` / `RankManager.*` member calls are not resolving to callable members at all.
2. Each unresolved call still pays:
   - cross-file member resolution fallback;
   - and/or `type_of_expr(prefix)` + `member_type` on a large class type.
3. Once one large type expression is inferred, later calls are cheaper, but the repeated unresolved-member fallback is still ~1.5-2ms per call.

## Code audit findings

### Fixed: O(n) member scans in `resolve_member`

`resolve_member_impl` Stage 3 / Stage 3.5 used to scan all members of an owner to find one name:

```rust
for member_ref in self.members_of_owner(&resolved) {
    if member_ref.name == name { ... }
}
```

Changed to use the already-existing name index:

```rust
for member_ref in self.members_of_owner_named(&resolved, name.as_str()) { ... }
```

Also applied to the runtime `self.xxx` fallback.

Measured effect on `player/player.lua`:

- Before: ~2.01-2.04s
- After: ~2.03-2.27s (no clear improvement)

So this removes an obvious O(n) scan, but the dominant unresolved-member cost is elsewhere.

### Added: `WorkspaceMemberIndex.by_owner_name`

`WorkspaceMemberIndex` previously only had:

```rust
by_owner: HashMap<SemanticId, Arc<[MemberRef]>>
```

`members_of_owner_named` for non-file-local owners therefore fetched the owner's full member array and filtered it linearly:

```rust
members.iter().filter(|member| member.name == name)
```

This is O(members-of-owner) per lookup.

Added:

```rust
by_owner_name: HashMap<(SemanticId, SmolStr), Arc<[u32]>>
```

where the values are indices into `by_owner[owner]`. This turns the non-file-local named-member lookup into a hash lookup without duplicating `MemberRef` payloads.

Measured on `player/player.lua`:

- Before: ~2.01-2.04s
- After: ~2.22-2.32s

No clear improvement on this sample. The unresolved-member path is still dominated by later cross-file fallback, not by this named-member scan.

### Remaining suspicious areas

1. **`resolve_member` unresolved fallback**
   - When a member is not found, `resolve_member_impl` still walks cross-file owners / runtime-value owners.
   - For many `pPlayer.*` calls, this costs ~1.5-2ms each.
   - Likely candidates for a cache:
     - `(owner, member_name) -> not found`
     - `(owner, member_name) -> resolved member`
   - This should be workspace-level or model-level, not per index-expression only.

2. **`member_callable_candidates` fallback when `member_id` is None**
   - It calls `type_of_expr(prefix)` and `member_type(prefix_ty, key)` even when `resolve_member` already returned no member.
   - For a huge class/table type, this can be very expensive (one observed 83ms).
   - Need a fast negative when the prefix type is a named class/table and the member is already known to be absent.

3. **Eager argument type inference in `call_site_analysis`**
   - Non-empty candidate calls spend ~240ms inferring all argument types.
   - This is needed for parameter checks, but may be avoidable for calls that can be rejected by candidate count/shape before full argument inference.

4. **Cross-file member resolution correctness**
   - Many unresolved calls (`pPlayer.Msg`, `pPlayer.GetTask`, etc.) may actually be valid methods defined cross-file.
   - Fixing cross-file member resolution would turn many empty-candidate calls into normal calls, reducing both false diagnostics and wasted fallback work.

## Cross-file unresolved members: root cause found (meta API classes)

A large part of the unresolved `pPlayer.*` / `RankManager.*` calls was not a
performance issue but a semantic resolution bug.

Example from real project:

```lua
-- serverLuaApi_gs/_KLuaPlayer_anotation.lua (---@meta)
---@class _KLuaPlayer
_KLuaPlayer = {}
function _KLuaPlayer.Msg(szMsg) end
```

```lua
-- alias
---@alias KLuaPlayer _KLuaPlayer
```

```lua
-- usage
local pPlayer = KPlayer.GetPlayerObjById() -- returns KLuaPlayer
pPlayer.Msg("hello")
```

`pPlayer.Msg` was unresolved because:

- The class `_KLuaPlayer` is a `TypeDef`;
- The runtime table `_KLuaPlayer = {}` is a global `Decl`;
- The method `function _KLuaPlayer.Msg` is stored under `SemanticId::Name("_KLuaPlayer")`;
- `resolve_owner_set(TypeDef)` returned only `TypeDef` + `Decl`, but **not** `Name`.

So the class type never saw members declared on the same-name global table in
main-workspace `---@meta` API files.

### Fix

`resolve_owner_set` now includes `SemanticId::Name(bare_name)` when:

- the declaring file is a main-workspace `---@meta` file;
- the same-name runtime value is a global declaration.

This is intentionally limited to main-workspace meta files to avoid changing std/remote/library
meta behavior (which caused regressions such as `string` extension methods and pcall inference).

### Regression test

Added `test_global_runtime_class_members_cross_file` in
`crates/emmylua_code_analysis/src/check/test/undefined_field_test.rs`.

Measured performance on `player/player.lua` after the fix:

- ~2.13-2.16s

No clear end-to-end time improvement on this sample yet, but the semantic correctness issue
is fixed and should reduce false `UndefinedField` / unresolved callable reports.

## Next steps / suggested experiments
## Next steps / suggested experiments

1. Add an unresolved-member negative cache keyed by `(owner, member_name)`.
2. Profile `resolve_member_impl` stages separately:
   - Stage 3 direct owner lookup
   - Stage 3 runtime self-member fallback
   - Stage 4 cross-file runtime member + require module fallback
3. Investigate `type_of_expr(prefix)` / `member_type` cost for large types in the `member_id == None` fallback.
4. Consider lazily computing `arg_types` in `call_site_analysis` and only materializing them when a candidate actually needs full parameter checking.
5. Fix cross-file method resolution for common runtime/class patterns; this may remove a large portion of empty-candidate work entirely.

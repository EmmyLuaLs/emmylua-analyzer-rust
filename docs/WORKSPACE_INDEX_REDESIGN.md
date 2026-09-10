# Workspace Index 与跨文件语义重构计划

> 状态：P0/P1/P2/P3/P4 已完成，P5 待开始
> 目标读者：后续接手 `semantic_db` / `semantic_model` 的开发者
> 维护方式：本文件随每个阶段实时更新；已完成项必须附测试名和验证命令。

## 0. 当前状态

| 阶段 | 内容 | 状态 |
|---|---|---|
| P0 | 行为基线、跨文件/重载失败用例、增量一致性测试、重建计数器 | ✅ 已完成 |
| P1 | canonical identity + `FileExportContribution` | ✅ 已完成 |
| P2 | shard per-file 化，消灭 `build_*_shard` 全量扫描 | ✅ 已完成 |
| P3 | 四个 workspace index 改为增量聚合 | ✅ 已完成 |
| P4 | 重写单文件更新路径，删除单文件写入中的全量 rebuild | ✅ 已完成 |
| P5 | canonical owner 解析 + require 深层链 | ⬜ 待开始 |
| P6 | 统一 callable/overload 候选与选择 | ⬜ 待开始 |
| P7 | 身份级依赖失效，删除字符串级 `SurfaceDelta` | ⬜ 待开始 |

---

## 1. 审计结论：当前为什么“只有形，没有实”

### 1.1 workspace index 的构建仍在全量扫文件

`FileFacts -> FileExports -> ExportShard -> WorkspaceIndex` 表面上分层，但以下 shard builder 都遍历 `db.file_ids()`：

- `exports::build_export_shard()`
- `query::build_deprecated_shard()`
- `query::build_module_shard()`
- `query::build_reference_shard()`

单文件更新时重建一个 shard 的成本是 `O(文件总数)`；初始构建是 `O(64 * 文件总数)`。

### 1.2 `ExportShard` 没有单文件 contribution，无法做移除/替换

```rust
pub struct ExportShard {
    pub types: Vec<TypeDef>,
    pub globals: Vec<GlobalExport>,
    pub runtime_values: Vec<(FileId, SmolStr, SemanticId)>,
    pub members: Vec<MemberExport>,
    pub modules: Vec<(FileId, ModuleExport)>,
}
```

没有 `FileId -> contribution`，因此无法：

- 只移除旧文件贡献；
- 只加入新文件贡献；
- 只更新受影响的 key。

只能重新遍历所有文件。

### 1.3 单文件写入路径仍会全量重建

`query::rebuild_file_after_write()` 中：

```rust
let needs_global_rebuild =
    metadata_changed || exports_changed || module_surface_changed || old_exports.is_none();

if needs_global_rebuild {
    ...
    rebuild_workspace_indexes(db);
    ...
} else {
    ...
    rebuild_workspace_reference_indexes(db);
}
```

- `rebuild_workspace_indexes()` 重建 types/members/decls/modules 全部工作区索引。
- `rebuild_workspace_reference_indexes()` 重建全部工作区 reference index。
- 即使 export surface 不变，也会调用 `build_reference_shard()`，仍扫描全部文件。

### 1.4 `SurfaceDelta` 是字符串级粗粒度依赖

```rust
struct SurfaceDelta {
    names: HashSet<SmolStr>,
    member_names: HashSet<SmolStr>,
}
```

只比较全局名/类型名/member 名，不比较：

- `(TypeScope, full_name)` 身份；
- 具体 global decl 身份；
- `(OwnerId, LuaMemberKey)` 下的定义集合；
- module export 身份；
- runtime value 映射。

因此既可能漏失效，也可能过度失效。

### 1.5 跨文件 owner 解析是启发式合并 + 打分

`resolve_owner_set()` 把 `Name`、同名 TypeDef、同名全局 Decl、同名 runtime decl、owner_syntax 关联 def、名称链 member 全部塞进一个 `Vec<SemanticId>`，再由 `resolve_member_impl()` 用 score 猜：

```rust
let mut score = if is_old { 10_000 } else { 0 };
if member_facts.visibility != VisibilityKind::Public { score -= 2_000; }
...
if best.as_ref().is_none_or(|(best_score, _)| score < *best_score) {
    best = Some((score, member));
}
```

这不是确定性语义解析。

### 1.6 `require` 深层链和跨文件 module 修改会断

`require_module_owner()` 只接受 `NameExpr` 前缀：

```rust
let LuaExpr::NameExpr(name_expr) = prefix else {
    return None;
};
```

后果：

- `local M = require("mod"); M.foo` 可能通过特判工作；
- `local M = require("mod"); M.sub.foo` 失效；
- `local M = require("mod"); M.foo = 1` 会挂到 consumer 本地 `Decl(M)`，而不是 module export owner；
- `local M = require("mod"); local N = M; N.foo` 无法传递 module owner。

### 1.7 重载被压成单个 `LuaType` / `MemberInfo`

当前存在多套不一致的 callable candidate 路径：

- `param_type_check::callable_candidates_uncached()`
- `param_type_check::member_callable_candidates()`
- `InferVm::callable_candidates()` / `signature_candidates()`
- `param_count::callable_functions()`
- `member_infos()` / `type_of_member()`

关键问题：

- `member_infos()` 用 `dedup_by_key()` 按 key 去重，天然丢掉 overload；
- `member_callable_candidates()` 拿到精确 member 后提前返回，不再收集同 owner/name 的其他定义；
- VM 的 `signature_candidates()` 只展开单个声明里的 `---@overload`，不展开多个同名声明；
- 诊断、VM、补全、hover 各走各的路径，结果不可能一致。

---

## 2. 目标架构

```text
VFS / SyntaxTree
    ↓
FileFacts                 单文件事实，可重建，不做跨文件推断
    ↓
FileExportContribution    单文件贡献，带稳定 key / 源顺序
    ↓
WorkspaceIndexLayer       可增量更新的工作区索引
    ↓
SemanticModel queries     只读查询，不扫描文件、不重建索引
```

核心原则：

1. workspace index 由单文件 contribution 组成，维护 `key -> aggregate` 和 `file_id -> contribution keys`。
2. 单文件更新 = 移除旧 contribution + 加入新 contribution，不扫描其他文件。
3. 跨文件身份使用 canonical `OwnerId` / `ExportKey`，不再用 raw `SemanticId` + 启发式合并。
4. 重载以候选集合存在，从事实层到查询层都不能压成单个 `LuaType`。
5. 依赖失效基于身份和 key，不基于字符串名字。
6. `LuaSignatureId` 是合理类型，不作为清理目标。

---

## 3. 分阶段计划

### P0：行为基线与测试（已完成）

目标：

- 把当前跨文件/重载/增量更新的错误行为固化为测试；
- 建立“增量更新结果 == 全量重建结果”的正确性守卫；
- 为后续阶段提供可量化的重建次数基线。

已完成内容：

- 新增 `crates/emmylua_code_analysis/src/semantic_db/p0_tests.rs`。
- 新增 test-only `RebuildMetrics`（仅 `#[cfg(test)]`）：
  - `full_rebuilds`
  - `workspace_index_rebuilds`
  - `shard_scan_builds`
- 在 `rebuild_all_caches` / `rebuild_workspace_indexes` / 四个 shard builder 中计数。
- 通过测试：
  - `p0_incremental_module_member_edit_matches_full_rebuild`
  - `p0_incremental_multi_edit_matches_full_rebuild`
  - `p0_deep_require_member_chain`
  - `p0_surface_preserving_edit_does_not_scan_all_files`（P2 后启用）
  - `p0_export_changing_edit_does_not_rebuild_workspace_indexes`（P3 后启用）
- 已知失败基线（`#[ignore]`，作为 P5/P6 验收标准）：
  - `p0_cross_file_module_mutation_visible_through_require`
    - 当前失败：返回 `Unknown`，期望模块 member 可见。
  - `p0_cross_file_global_overloads`
    - 当前失败：字符串调用返回 `Unknown`，期望考虑两个同名全局函数声明。

运行方式：

```bash
# 常规 CI：P0 回归守卫
cargo test -p emmylua_code_analysis p0_tests

# 查看当前已知失败基线
cargo test -p emmylua_code_analysis p0_tests -- --ignored --nocapture
```

当前验证结果（2026-09-09）：

```text
cargo test -p emmylua_code_analysis p0_tests
  5 passed; 0 failed; 2 ignored

cargo test -p emmylua_code_analysis --lib
  1329 passed; 0 failed; 4 ignored

cargo clippy --workspace --all-targets -- -D warnings
  passed

cargo test --workspace --lib
  passed
```

验收标准：

- 常规测试全绿；
- 忽略测试必须按阶段逐个启用，不能删除。

### P1：canonical identity + `FileExportContribution`（已完成）

已完成内容：

- 新增 `crates/emmylua_code_analysis/src/semantic_db/def/identity.rs`：
  - `OwnerId`：`Type` / `Global` / `Module` / `Local` / `Table` / `Concrete`（过渡）。
  - `ExportKey`：`Type` / `Global` / `RuntimeValue` / `Member` / `Module`。
- `FileExports` 改为 `FileExportContribution`，并保留 `pub type FileExports = FileExportContribution` 兼容别名。
- `FileCache.exports` 改为 `Arc<FileExportContribution>`，避免每次更新深拷贝。
- `MemberExport` 扩展为：
  - `owner`（raw，兼容旧查询）
  - `owner_id`（canonical）
  - `key` / `member`
  - `value_syntax`
  - `is_method`
  - `visibility`
  - `deprecated`
  - `order`（源顺序）
- `FileExportContribution::surface_eq()` 用于更新路径：member 的 `value_syntax` 不参与 surface 比较，避免 `M.x = 1` -> `M.x = 100` 这种纯值编辑误触发 workspace index 重建。
- `surface_delta()` 的 member 比较改用 `(ExportKey, SemanticId)` + `MemberExport::surface_eq()`；concrete member id 保证 overload 不会在 delta 阶段被合并。
- overload 不合并：每个 `@field f fun(...)` / 同名声明都是独立 `MemberExport`。

新增测试：

- `crates/emmylua_code_analysis/src/semantic_db/p1_tests.rs`
  - `p1_member_contribution_carries_canonical_owner_and_flags`
  - `p1_member_contribution_preserves_overloads_and_export_key`
  - `p1_surface_eq_ignores_member_value_syntax_changes`
  - `p1_file_cache_stores_contribution_as_arc`

运行方式：

```bash
cargo test -p emmylua_code_analysis p1_tests
```

验收结果：

```text
cargo test -p emmylua_code_analysis p1_tests
  4 passed; 0 failed

cargo test -p emmylua_code_analysis --lib
  1316 passed; 0 failed; 6 ignored

cargo clippy --workspace --all-targets -- -D warnings
  passed
```

### P2：shard per-file 化（已完成）

已完成内容：

- `SemanticDatabase` 增加：
  - `shard_files: Vec<Vec<FileId>>`：稳定 shard -> file id 列表；
  - `module_fallback_root: Option<PathBuf>`：无 workspace root 时的模块名根目录缓存。
- `ExportShard` 改为 `files: HashMap<FileId, Arc<FileExportContribution>>`。
- `DeprecatedShard` 改为 `files: HashMap<FileId, DeprecatedFileData>`。
- `ModuleShard` 改为 `files: HashMap<FileId, ModuleEntry>`。
- `ReferenceShard` 改为 `files: HashMap<FileId, Arc<FileReferences>>`。
- `FileCache.references` 改为 `Arc<FileReferences>`，与 contribution 一样可被 shard 共享。
- 所有 `build_*_shard` 只遍历 `db.file_ids_in_shard(shard)`，不再遍历 `db.file_ids()`。
- 单文件写入：
  - 只替换 `shard.exports.files[file_id]`；
  - 只更新该文件的 deprecated/module/reference entry；
  - 不再重建整个 shard。
- 单文件删除：只从对应 shard 的 per-file map 中移除该 file entry。
- `update_main_root()` 会触发一次全量 rebuild，以刷新 `module_fallback_root` 与 module shard/index。

新增测试：

- `crates/emmylua_code_analysis/src/semantic_db/p2_tests.rs`
  - `p2_export_shard_is_per_file`
  - `p2_shard_file_lists_are_maintained_incrementally`
  - `p2_module_and_deprecated_shards_are_per_file`

P0 启用：

- `p0_surface_preserving_edit_does_not_scan_all_files` 已从 `#[ignore]` 启用并通过。

验收结果：

```text
cargo test -p emmylua_code_analysis p2_tests
  3 passed; 0 failed

cargo test -p emmylua_code_analysis p0_tests
  4 passed; 0 failed; 3 ignored

cargo test -p emmylua_code_analysis --lib
  1320 passed; 0 failed; 5 ignored

cargo clippy --workspace --all-targets -- -D warnings
  passed
```

### P3：workspace index 增量聚合（已完成）

已完成内容：

- `WorkspaceTypeIndex`：
  - `by_scope_name: (TypeScope, full_name) -> Vec<TypeDef>`；
  - `by_file: FileId -> Vec<(key, SemanticId)>`；
  - `remove_file` / `add_file`。
- `WorkspaceMemberIndex`：
  - `OwnerMembers { by_id, by_name, order }`；
  - `by_name` / `order` 保留 overload 源顺序；
  - `by_file: FileId -> Vec<(owner, member_id)>`；
  - `members_of_owner` / `members_of_owner_named` 从 bucket 生成 Arc 结果。
- `WorkspaceDeclIndex`：
  - `global_by_name: HashMap<SmolStr, Vec<SemanticId>>`；
  - `runtime_by_name` / `runtime_by_file_name` / `type_def_by_id`；
  - `by_file: FileId -> FileDeclContribution`。
- `ModuleIndex`：
  - 增加 `workspace_id`；
  - `apply_file_change(file_id, new_entry)` 只重建该 workspace 的 entries/derived tree。
- `WorkspaceReferenceIndex`：
  - 保留 aggregate `decl_refs/member_refs/member_defs`；
  - 增加 `by_file: HashMap<FileId, Arc<FileReferences>>`；
  - `remove_file` / `add_file` 按 target 精确移除/加入该文件的范围。
- 单文件更新路径：
  - `rebuild_file_after_write()` 在非 metadata 变化时调用 `apply_file_to_workspace_indexes()`；
  - `rebuild_reference_indexes_incremental()` 只更新受影响文件的 workspace reference index；
  - 不再调用 `rebuild_workspace_indexes()` / `rebuild_workspace_reference_indexes()`。

新增测试：

- `crates/emmylua_code_analysis/src/semantic_db/p3_tests.rs`
  - `p3_type_index_remove_add_matches_full_rebuild`
  - `p3_member_index_remove_add_preserves_overloads`
  - `p3_reference_index_remove_add_matches_full_rebuild`
  - `p3_module_index_apply_file_change_is_workspace_local`

P0 启用：

- `p0_export_changing_edit_does_not_rebuild_workspace_indexes` 已从 `#[ignore]` 启用并通过。

验收结果：

```text
cargo test -p emmylua_code_analysis p3_tests
  4 passed; 0 failed

cargo test -p emmylua_code_analysis p0_tests
  5 passed; 0 failed; 2 ignored

cargo test -p emmylua_code_analysis --lib
  1329 passed; 0 failed; 4 ignored

cargo clippy --workspace --all-targets -- -D warnings
  passed
```

### P4：重写单文件更新路径（已完成）

已完成内容：

- `rebuild_file_after_write()` 不再有 full rebuild 分支：
  - 构建 new facts / exports / references；
  - 先更新 export/type/member/decl/module workspace indexes；
  - 再解析该文件 references，避免新文件看不到自己的声明；
  - 最后更新 reference index / shard entry；
  - 用 `surface_delta` 只刷新依赖变化名字/成员的其他文件。
- 文件新增：`old_exports.is_none()` 走 contribution add，不再全量 rebuild。
- 文件删除：
  - `workspace_remove_file()` 保留 old workspace / old exports；
  - `rebuild_file_after_remove()` 从 workspace indexes / shards 中精确移除；
  - 只刷新依赖被删除名字/成员的其他文件。
- `metadata_changed` / module surface 变化：
  - 通过 `apply_file_change()` 更新该 workspace 的 module index；
  - 只有“无 roots 且无 main_root”导致 common fallback root 变化时，才重建 module shards/indexes。
- 全量 `rebuild_all_caches()` 现在只用于初始加载、config/roots 变化、`clear()`、测试。
- `rebuild_reference_indexes()` 已删除；`rebuild_workspace_indexes()` 只由 full rebuild 使用。

新增测试：

- `crates/emmylua_code_analysis/src/semantic_db/p4_tests.rs`
  - `p4_new_file_does_not_full_rebuild`
  - `p4_remove_file_does_not_full_rebuild`
  - `p4_new_file_refreshes_dependent_references`
  - `p4_path_change_does_not_full_rebuild`

验收结果：

```text
cargo test -p emmylua_code_analysis p4_tests
  4 passed; 0 failed

cargo test -p emmylua_code_analysis p0_tests
  5 passed; 0 failed; 2 ignored

cargo test -p emmylua_code_analysis --lib
  1329 passed; 0 failed; 4 ignored

cargo clippy --workspace --all-targets -- -D warnings
  passed
```

### P5：canonical owner 解析

- 用 `resolve_owner_ids()` 替换 `resolve_owner_set()` + score。
- `require` 初始化建立 `RequireAliasContribution`，所有 `M.foo` / `M.sub.foo` / `M.foo = ...` 都走 `OwnerId::Module`。
- 多 workspace 优先级显式化，不再“找到第一个就返回”。

### P6：统一 callable/overload

- 新增 `CallableCandidateSet`：
  - 同名 decl/member 的所有定义；
  - 每个定义的 `---@overload`；
  - `---@operator call`；
  - repeated `---@field`；
  - 继承链候选；
  - generic 参数与 colon/dot 信息。
- VM、诊断、补全、hover、signature help 统一走 `select_callable` / `select_callable_all`。
- 删除 `member_callable_candidates()` 等启发式分支。
- 明确 overload 合并与 tie-breaker 规则。

### P7：身份级依赖失效

- `FileDependencies { globals, types, members, modules }`。
- workspace index 更新返回 `ChangedKeys`。
- 只失效 `deps.intersects(changed_keys)` 的文件。
- 删除 `SurfaceDelta` 和字符串级 `name_deps` / `member_name_deps`。

---

## 4. 验收标准

1. 单文件写入不再调用 `rebuild_all_caches` / `rebuild_workspace_indexes`。
2. 单文件写入复杂度与文件 contribution 大小相关，与工作区文件总数无关。
3. `build_*_shard` 不再遍历 `db.file_ids()`。
4. `require` 深层链、require 别名传递、跨文件 module 修改全部正确。
5. 所有 overload 场景在诊断、VM return inference、hover、signature help、completion 中一致。
6. `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings` 全绿。
7. P0 的 ignored 用例逐个启用并通过。

---

## 5. 更新日志

- 2026-09-09：P4 完成。
  - 文件新增/删除/metadata 变化全部走增量 contribution remove/add。
  - 删除 `rebuild_reference_indexes()`；全量 rebuild 仅保留给初始加载/config/roots/clear/测试。
  - 新增 `p4_tests.rs`（4 个测试）。
  - 验证：`cargo test -p emmylua_code_analysis --lib`（1329 passed, 4 ignored）、
    `cargo clippy --workspace --all-targets -- -D warnings`。
- 2026-09-09：P3 完成（P4 同步推进）。
  - 四个 workspace index 增加 per-file contribution 与 remove/add API。
  - 单文件写入改为 `apply_file_to_workspace_indexes()`，不再调用
    `rebuild_workspace_indexes()` / `rebuild_workspace_reference_indexes()`。
  - 新增 `p3_tests.rs`（4 个测试）。
  - P0 `p0_export_changing_edit_does_not_rebuild_workspace_indexes` 已启用并通过。
  - 验证：`cargo test -p emmylua_code_analysis --lib`（1329 passed, 4 ignored）、
    `cargo clippy --workspace --all-targets -- -D warnings`。
- 2026-09-09：P2 完成。
  - shard 改为 per-file map；`build_*_shard` 不再遍历 `db.file_ids()`。
  - `FileCache.references` 改为 `Arc<FileReferences>`。
  - 新增 `p2_tests.rs`（3 个测试）。
  - P0 `p0_surface_preserving_edit_does_not_scan_all_files` 已启用并通过。
  - 验证：`cargo test -p emmylua_code_analysis --lib`（1320 passed, 5 ignored）、
    `cargo clippy --workspace --all-targets -- -D warnings`。
- 2026-09-09：P1 完成。
  - 新增 `OwnerId` / `ExportKey`。
  - `FileExports` 改为 `FileExportContribution`，`FileCache` 以 `Arc` 持有。
  - `MemberExport` 增加 `owner_id/value_syntax/is_method/visibility/order`。
  - 新增 `p1_tests.rs`（4 个测试）。
  - 验证：`cargo test -p emmylua_code_analysis --lib`（1316 passed, 6 ignored）。
- 2026-09-09：P0 完成。
  - 新增 `p0_tests.rs`。
  - 新增 test-only `RebuildMetrics`。
  - 当前已知失败基线 4 个，均已标注 `#[ignore]` 和原因。
  - 验证：`cargo test -p emmylua_code_analysis --lib`（1312 passed, 6 ignored）、
    `cargo clippy --workspace --all-targets -- -D warnings`。

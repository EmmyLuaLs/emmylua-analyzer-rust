# Workspace Index 与跨文件语义重构计划

> 状态：P0-P7 全部完成；`resolve_owner_set()` 已是 `resolve_owner_ids()` 的确定性 wrapper，不再有启发式 score。
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
| P4.5 | 查询期性能止血：keyed member lookup + flow 索引 + 局部热路径 | ✅ 已完成 |
| P5 | canonical owner 解析 + require 深层链 | ✅ P5a/P5b 已完成 |
| P6 | 统一 callable/overload 候选与选择 | ✅ 已完成 |
| P7 | 身份级依赖失效，删除字符串级 `SurfaceDelta` | ✅ 已完成 |

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

### 1.5 跨文件 owner 解析是启发式合并 + 打分（已修复：P5b/P6/P7）

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

### 1.6 `require` 深层链和跨文件 module 修改会断（已修复：P5）

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

### 1.7 重载被压成单个 `LuaType` / `MemberInfo`（已修复：P6）

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

### P4.5：查询期性能止血（已完成）

背景：P0–P4 解决了索引层的全量扫描，但单文件诊断仍存在查询期 O(N²) 热路径。
commit 前实测（release、Windows、`tools/perf/generate_synthetic_workspace.py` 生成的
1201 文件 workspace，用 `bench_file` 测单文件 `diagnose_file`）：

| 场景 | 修复前 | P4.5 后 |
|---|---:|---:|
| `local M = {}; M.x_i = i` × 1200 | 5.50s | 0.06s |
| `M.x_i = require(...)` × 1200 | 7.96s | 0.24s |
| 聚合文件（1200 require + 3600 次成员/方法访问） | 10.92s | 1.3s |
| 全 workspace check（1201 文件，含冷启动） | 11.41s | 2.8s |

profile 中 `AssignTypeMismatchChecker` 从 7.16s 降到约 60ms，`NeedCheckNilChecker`
从 706ms 降到约 28ms。

已完成内容：

1. **keyed member lookup（P4.5a）**
   - `semantic_model/member.rs`：新增 `owner_member_refs()`，`LuaMemberKey::Name`
     查询走 owner/name bucket，不再 `collect_members()` 全量枚举后再过滤。
   - `member_infos_with_key()` / `member_infos_with_key_all()` 改为 keyed 收集，
     overload / 继承 / runtime value / union 语义保持不变。
   - 新增 test-only `query_metrics::FULL_OWNER_MEMBER_SCANS` 作为回归守卫。
2. **Flow 查询索引与 fast path（P4.5b）**
   - `semantic_db/flow/flow_tree.rs` 新增：
     - `decl_assignments` / `member_assignments`（按位置排序，二分找最近一次赋值）
     - `decl_positions`
     - `condition_nodes` / `cast_nodes` / `branch_nodes`
   - `semantic_model/flow.rs`：
     - `type_of_decl_assign_target_at()`：无 cast 时直接返回声明类型（ASSIGN_TARGET
       本来就忽略赋值和 guard），字段赋值不再 O(offset) 回溯。
     - `type_of_decl_at()`：声明后无赋值且范围内无 condition/cast/branch merge 时
       直接返回声明类型。
     - `type_of_member_at()`：范围内无 flow event 时从最近一次成员赋值直接求值。
   - 新增 test-only `flow_metrics`：`TRACE_STEPS` + fast path hit 计数。
3. **RedefinedLocal leaf scope（P4.5c）**
   - `check/checker/redefined_local.rs`：叶子 scope 且 `should_merge` 时直接在
     parent map 上插入，不再每个 `local` 语句 clone 整个 map。
   - 新增 test-only `scope_metrics::CLONED_LOCAL_ENTRIES` 回归守卫。
4. **依赖刷新 early return**
   - `semantic_db/query.rs::rebuild_dependent_reference_indexes()`：`SurfaceDelta`
     为空时直接返回，纯 value edit 不再遍历 `db.files`。
5. **可复现 benchmark**
   - `tools/perf/generate_synthetic_workspace.py` 生成上述 workspace。

新增测试：

- `semantic_db/p4_5_tests.rs`
  - `p4_5_keyed_member_lookup_uses_owner_name_bucket`
  - `p4_5_keyed_member_lookup_keeps_overloads`
  - `p4_5_flow_reads_use_indexed_fast_paths`
  - `p4_5_flow_does_not_shortcut_loop_or_branch_member_reads`
  - `p4_5_redefined_local_leaf_scopes_do_not_clone_parent_map`

运行方式：

```bash
cargo test -p emmylua_code_analysis p4_5_tests
cargo test -p emmylua_code_analysis --lib
cargo clippy --workspace --all-targets -- -D warnings

python tools/perf/generate_synthetic_workspace.py target/perfws 1200
cargo build --release -p emmylua_check --bin bench_file
target/release/bench_file target/perfws target/perfws/main.lua
```

验收结果（2026-09-10）：

```text
cargo test -p emmylua_code_analysis p4_5_tests
  5 passed; 0 failed

cargo test -p emmylua_code_analysis --lib
  1334 passed; 0 failed; 4 ignored

cargo clippy --workspace --all-targets -- -D warnings
  passed
```

已知剩余：聚合文件 main_1200 仍随 K 约 2.6x/倍增，profile 显示
`ParamTypeChecker` / `AccessInvisibleChecker` / `UnusedChecker` 等仍有轻微超线性；
P5（owner/require 身份）与 P7（身份级依赖）完成后继续复测。

### P5：canonical owner 解析（P5a/P5b 已完成）

P5a 已完成内容：

- `exports.rs` 新增 `RequireAliasContribution`：
  - `local M = require("mod")` -> `M` 声明映射到 `OwnerId::Module(mod_file)`；
  - 支持 alias 链 `local N = M`（固定点解析）和括号；
  - `FileExportContribution.aliases` 参与 surface 比较与增量更新。
- `build_file_exports()` 的 member `owner_id` 规范化：
  - 模块 export target 自身的成员 -> `OwnerId::Module(file_id)`；
  - `M.foo = ...` / `function M.foo()` 如果 `M` 是 require alias -> `OwnerId::Module(target_file)`。
- `WorkspaceMemberIndex` 增加 canonical `OwnerId` 桶：
  - `members_of_owner_id()` / `members_of_owner_id_named()`；
  - `members_of_owner()` / `members_of_owner_named()` 在 raw facts 结果上合并 canonical bucket，
    因此 `require("mod").extra()` 能看到其他文件对 module 的 mutation。
- `rebuild_all_caches()` 调整顺序：module entries/index 在 exports 之前构建，
  保证 batch/full rebuild 时 alias 解析可用。
- `require_module_owner()` 泛化：
  - 支持直接 `require("mod").foo` 前缀；
  - 支持 `local M = require(...)` / `local N = M` alias 链（带深度限制）。
- `CheckExportChecker` 明确区分 module 原始 export surface 和其他文件 canonical mutation：
  - 跨文件 mutation 对语义解析可见；
  - 但不降低 `InjectField` / `UndefinedField` 对消费者的判断。

启用 P0 用例：

- `p0_cross_file_module_mutation_visible_through_require` 已启用并通过。

新增测试：

- `semantic_db/p5_tests.rs`
  - `p5_require_alias_contribution_maps_to_module_owner`
  - `p5_require_alias_chain_attaches_mutation_to_module_owner`
  - `p5_require_alias_contribution_survives_batch_rebuild`

验收结果（2026-09-10）：

```text
cargo test -p emmylua_code_analysis p5_tests
  3 passed; 0 failed

cargo test -p emmylua_code_analysis p0_tests
  6 passed; 0 failed; 1 ignored

cargo test -p emmylua_code_analysis --lib
  1338 passed; 0 failed; 3 ignored

cargo clippy --workspace --all-targets -- -D warnings
  passed
```

P5b 已完成内容：

- 新增确定性身份 API：
  - `resolve_owner_ids(&SemanticId) -> Vec<OwnerId>`；
  - `owner_id_to_semantic_id(&OwnerId) -> Option<SemanticId>`；
  - `canonical_owner_id(&SemanticId) -> Option<OwnerId>`；
  - facade 暴露 `resolve_owner_ids` / `owner_id_to_semantic_id`。
- `resolve_member_impl()` Stage 4：
  - 候选 owner 由 `resolve_owner_ids()` 统一产生，不再局部拼 `type_defs_in_scope` + `global_decl`；
  - 数值 score 替换为显式排序键：
    `(non-old-owner rank, non-public rank, doc rank, discovery index)`；
  - 行为保持等价，后续 P6 可以在此基础上直接替换为候选集合选择。
- require 深链 canonical path：
  - 新增 `require_module_member_owner_of_expr()`；
  - `M.sub.foo` / `require("mod").sub.foo` 会先解析 `sub` 的值表 owner，再解析 `foo`；
  - 仍保留 8 层深度限制。
- 多 workspace 优先级显式化：
  - 新增 `workspace_lookup_order()`：main > library > std > remote；
  - `module_file_of()` 使用该顺序，同名模块 main 优先于 library/std。

新增测试：

- `semantic_db/p5_tests.rs`
  - `p5b_resolve_owner_ids_is_deterministic`
  - `p5b_deep_alias_member_chain`
  - `p5b_main_workspace_wins_over_library_module_name`

验收结果（2026-09-10）：

```text
cargo test -p emmylua_code_analysis p5_tests
  6 passed; 0 failed

cargo test -p emmylua_code_analysis --lib
  1341 passed; 0 failed; 3 ignored

cargo test --workspace
  exit=0

cargo clippy --workspace --all-targets -- -D warnings
  passed
```

性能：P4.5 合成聚合文件 benchmark 约 1.2s（无回退）。

收尾：

- `resolve_owner_set()` 已改为 `resolve_owner_ids()` 的确定性 wrapper；
- 通过补回 Name / owner_syntax / runtime 关联顺序，修掉了之前的语义回归；
- 全量 lib / workspace 测试通过。
### P6：统一 callable/overload（P6a 已完成，P6b 待开始）

P6a 已完成内容：

- 跨文件全局 overload 进入统一候选：
  - `WorkspaceDeclIndex::global_decls_named()` 暴露同名全局声明；
  - `global_decls_named_in_workspace()` / `global_decls_named_for_file()` / `global_decls_named_primary()`；
  - overload 只在同一 workspace 内合并，跨 workspace 按 main > library > std 取第一组；
  - 避免 main 的 `f` 与 std/library 的 `f` 混合导致行为变化。

- `InferVm::signature_candidates()` 增加跨文件 wrapper：
  - 全局声明会收集同 workspace 同名声明并拼接候选；
  - `signature_candidates_single()` 保留单身份投影，避免递归；
  - 相同内容的重复声明不会被误判为 overload（保留 legacy 单符号行为）。

- `SemanticModel::callable_candidates_for_name_expr()`：
  - 诊断/call-site 与 VM 共用同一份全局候选；
  - `param_type_check::callable_candidates_uncached()` 对 NameExpr 走该入口；
  - 仅当多个声明的候选集存在差异时才启用统一 overload 集合，单声明/重复声明继续 legacy。

启用 P0 用例：

- `p0_cross_file_global_overloads` 已启用并通过。

新增测试：

- `semantic_db/p6_tests.rs`
  - `p6_cross_file_global_overloads_are_visible_to_call_site_analysis`
  - `p6_cross_file_global_overloads_select_by_argument_type`
  - `p6_identical_duplicate_globals_keep_legacy_behavior`

验收结果（2026-09-10）：

```text
cargo test -p emmylua_code_analysis p0_tests
  7 passed; 0 failed; 0 ignored

cargo test -p emmylua_code_analysis p6_tests
  3 passed; 0 failed

cargo test -p emmylua_code_analysis --lib
  1345 passed; 0 failed; 2 ignored

cargo test --workspace
  exit=0

cargo clippy --workspace --all-targets -- -D warnings
  passed
```

性能：P4.5 合成聚合文件 benchmark 约 1.14-1.18s，无回退。

P6b 已完成内容：

- 新增统一的 `CallableCandidateSet`（`semantic_model/infer/callable.rs`）：
  - `from_prefix_type()`：同 key 的 repeated `---@field`、运行时成员、继承链成员；
  - `from_prefix_type_for_member()`：按 resolved member 的 owner 作用域过滤；
  - local owner 不跨文件合并同名成员，TypeDef / Global / Module 允许跨文件；
  - 通过 `expand_callable_types_in_model()` 复用 VM 的别名 / union / class `---@overload`
    / `---@operator call` 展开；
  - `select()` / `select_partial()` / `select_all()` 复用 `infer::overload` 的选择与
    tie-breaker。
- 接入诊断 call-site：
  - `param_type_check::member_callable_candidates()` 改为调用 `CallableCandidateSet`，
    删除原来的 `facts.members_named` / `member_type` 多段启发式；
  - repeated `@field` 现在会让 `call_site_analysis` 拿到全部候选，并按实参类型选择。
- 接入 call-site 渲染：
  - `types.rs::inferred_call_doc_function()` 的 member overload 路径改用
    `CallableCandidateSet`（hover / completion / pcall 展示共用）。
- VM 的重复 `@field` 路径原本已通过 union 展开正确工作，保持不变；P6c 再统一到
  `select_all`。

新增测试：

- `semantic_db/p6_tests.rs`
  - `p6b_repeated_field_overloads_select_by_arguments`
  - `p6b_member_call_site_candidates_include_all_field_overloads`
  - `p6b_inferred_call_doc_function_selects_matching_field_overload`
  - `p6b_member_overload_diagnostics_match_by_argument_type`
  - `p6b_callable_candidate_set_selects_and_returns_all_matches`

验收结果（2026-09-10）：

```text
cargo test -p emmylua_code_analysis p6_tests
  8 passed; 0 failed

cargo test -p emmylua_code_analysis --lib
  1350 passed; 0 failed; 2 ignored

cargo test --workspace
  exit=0

cargo clippy --workspace --all-targets -- -D warnings
  passed
```

性能：P4.5 合成聚合文件 benchmark 约 1.15-1.27s，无回退。

P6c 已完成内容：

- 公开统一 candidate API：
  - `SemanticModel::callable_candidates_for_expr()`；
  - 内部复用诊断候选路径 + VM `signature_candidates`（main + `---@overload`）。
- LS 迁移：
  - `signature_helper/build_signature_helper.rs` 优先使用统一 API；
  - `completion/providers/function_provider.rs::callable_candidates()` 优先使用统一 API，
    旧逻辑仅作为空候选时的 fallback。
- VM 迁移：
  - union-return / callback-return 路径改用 `CallableCandidateSet::select_all()`；
  - `index_member` 的 repeated `@field` 合并路径改用 `CallableCandidateSet::from_prefix_type()`。
- 清理：
  - `param_type_check::member_callable_candidates()` 已删除，改为
    `index_expr_callable_candidates()` 且内部只走 `CallableCandidateSet`。
验收结果（2026-09-10）：

```text
cargo test -p emmylua_code_analysis --lib
  1350 passed; 0 failed; 2 ignored

cargo test -p emmylua_ls --lib
  214 passed; 0 failed

cargo test --workspace
  exit=0

cargo clippy --workspace --all-targets -- -D warnings
  passed
```

性能：合成聚合文件 benchmark 约 1.26-1.38s，与 P6b 基线相当。

收尾：

- `resolve_owner_set()` 已改为 `resolve_owner_ids()` 的确定性 wrapper；
- 之前的语义回归通过补回 Name / owner_syntax / runtime 关联顺序修复；
- 全量 lib / workspace 测试通过。
### P7：身份级依赖失效（已完成）

已完成内容：

- canonical 依赖 key：
  - `DependencyKey { Global, Type, RuntimeValue, Member, Module }`；
  - `FileDependencies`、`ChangedKeys` 类型；
  - member key 使用 `(OwnerId, LuaMemberKey)`，module/type/global 使用 canonical 身份。
- 每个文件的 `FileReferences` 新增 `deps: FileDependencies`：
  - NameExpr 解析目标 -> Global / Type / RuntimeValue / Member；
  - 未解析的全局名 -> Global(name)（后续文件补定义时仍可失效）；
  - member use -> `(owner_id, key)`；
  - `require("mod")` -> Module(module_file)。
- 反向依赖索引：
  - `SemanticDatabase.dependency_index: key -> {file_id}`；
  - full rebuild 后重建；
  - 文件写入/删除时先移除旧 deps、再加入新 deps。
- 增量失效：
  - `changed_keys(old_exports, new_exports)` 输出 canonical ChangedKeys；
  - `refresh_dependent_references()` 取 `dependency_index` 的并集，
    只刷新命中的其他文件；
  - 文件路径/metadata 变化时额外发布 Module(file)。
- 删除：
  - `SurfaceDelta`；
  - `FileReferences::name_deps` / `member_name_deps`；
  - 旧的 `rebuild_dependent_reference_indexes()` 全文件扫描。
新增测试：

- `semantic_db/p7_tests.rs`
  - `p7_dependency_index_records_identity_keys`
  - `p7_value_only_member_edit_has_no_changed_keys`
  - `p7_unrelated_member_edit_does_not_refresh_dependent_files`
  - `p7_changed_member_refreshes_only_intersecting_files`
  - `p7_global_dependency_refresh`

验收结果（2026-09-10）：

```text
cargo test -p emmylua_code_analysis p7_tests
  5 passed; 0 failed

cargo test -p emmylua_code_analysis --lib
  1355 passed; 0 failed; 2 ignored

cargo test --workspace
  exit=0

cargo clippy --workspace --all-targets -- -D warnings
  passed
```

性能：合成聚合文件 benchmark 约 1.17-1.21s，无回退。

收尾：

- `resolve_owner_set()` 已正式改为 `resolve_owner_ids()` 的确定性 wrapper；
- 不再有 score，也不再依赖字符串级名字扫描；
- P0-P7 全部测试与 clippy 通过。
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

- 2026-09-10：P8/收尾完成。
  - `resolve_owner_set()` 替换为 `resolve_owner_ids()` 的确定性 wrapper；
  - 补回 Name / owner_syntax / runtime 关联规则，消除此前的语义回归；
  - 删除最后的 score 逻辑；
  - 最终验证：`cargo test -p emmylua_code_analysis --lib`（1355 passed, 2 ignored）、
    `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、
    `cargo fmt --all -- --check` 全部通过。
- 2026-09-10：P7 完成（身份级依赖失效）。
  - 新增 `DependencyKey` / `FileDependencies` / `ChangedKeys`；
  - `FileReferences` 记录 canonical deps，`require` 记录 Module(file)；
  - `SemanticDatabase.dependency_index` 维护 key -> files 反向索引；
  - 增量写入改为 `changed_keys` + 依赖交集刷新，删除 `SurfaceDelta`、
    `name_deps` / `member_name_deps` 与全文件扫描；
  - 新增 `p7_tests.rs`（5 个测试）；
  - 验证：`cargo test -p emmylua_code_analysis --lib`（1355 passed, 2 ignored）、
    `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`。
- 2026-09-10：P6c 完成（LS/VM 统一候选路径）。
  - 新增公开的 `SemanticModel::callable_candidates_for_expr()`；
  - LS signature help / completion 优先使用统一 candidate API；
  - VM union-return 改用 `CallableCandidateSet::select_all()`，
    `overloaded_callable_member_type` 改用 `from_prefix_type()`；
  - 删除 `param_type_check::member_callable_candidates()`，改为
    `index_expr_callable_candidates()` + `CallableCandidateSet`；
  - 遗留 legacy `resolve_owner_set()` 少量调用点延后到 P7 清理；
  - 验证：`cargo test --workspace`、`cargo test -p emmylua_ls --lib`（214 passed）、
    `cargo clippy --workspace --all-targets -- -D warnings`。
- 2026-09-10：P6b 完成（member/@field/operator/继承链统一候选）。
  - 新增 `CallableCandidateSet`：`from_prefix_type` / `from_prefix_type_for_member`，
    复用 VM 的 alias/union/class overload/operator call 展开；
  - `param_type_check::member_callable_candidates()` 改为统一候选集并删除多段启发式；
  - `types.rs::inferred_call_doc_function()` member overload 路径改用统一候选集；
  - 新增 5 个 P6b 测试；
  - 验证：`cargo test -p emmylua_code_analysis --lib`（1350 passed, 2 ignored）、
    `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`。
- 2026-09-10：P6a 完成（跨文件全局 overload 统一）。
  - 新增 `global_decls_named_in_workspace` / `_for_file` / `_primary`：
    overload 只在同一 workspace 合并，跨 workspace 按 main > library > std 取第一组；
  - `InferVm::signature_candidates()` 增加跨文件 wrapper，`signature_candidates_single()`
    保留单身份投影；相同内容的重复声明仍走 legacy 单符号路径；
  - `SemanticModel::callable_candidates_for_name_expr()` 统一 NameExpr 候选，并接入
    `param_type_check`；
  - 启用 `p0_cross_file_global_overloads`；新增 `p6_tests.rs`（3 个测试）；
  - 验证：`cargo test -p emmylua_code_analysis --lib`（1345 passed, 2 ignored）、
    `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`。
- 2026-09-10：P5b 完成（确定性 owner 解析 + workspace 优先级）。
  - 新增 `resolve_owner_ids()` / `owner_id_to_semantic_id()` / `canonical_owner_id()`；
  - `resolve_member_impl()` Stage 4 候选 owner 改由 `resolve_owner_ids()` 产生，
    数值 score 替换为显式排序键；
  - `require_module_member_owner_of_expr()` 支持 `M.sub.foo` / `require("mod").sub.foo`；
  - `workspace_lookup_order()` 显式 main > library > std，`module_file_of()` 使用；
  - 新增 3 个 P5b 测试；
  - 遗留：`resolve_owner_set()` 全量替换触发 4 个语义回归，已回退，留待 P6；
  - 验证：`cargo test -p emmylua_code_analysis --lib`（1341 passed, 3 ignored）、
    `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`。
- 2026-09-10：P5a 完成（canonical module owner + require alias）。
  - 新增 `RequireAliasContribution` 与 `FileExportContribution.aliases`；
    `local M = require("mod")` / `local N = M` 的成员 contribution 落到
    `OwnerId::Module(target_file)`。
  - `WorkspaceMemberIndex` 增加 canonical `OwnerId` bucket，`members_of_owner[_named]`
    合并 raw facts 与 canonical module members。
  - `rebuild_all_caches()` 先构建 module entries/index，再构建 exports，保证 batch
    rebuild 时 alias 可解析。
  - `require_module_owner()` 支持直接 `require("mod")` 前缀和 alias 链。
  - 启用 `p0_cross_file_module_mutation_visible_through_require`；
    新增 `p5_tests.rs`（3 个测试）。
  - 验证：`cargo test -p emmylua_code_analysis --lib`（1338 passed, 3 ignored）、
    `cargo clippy --workspace --all-targets -- -D warnings`。
- 2026-09-10：P4.5 完成（查询期性能止血）。
  - keyed member lookup：`member_infos_with_key()` 不再全量枚举 owner 成员。
  - `FlowTree` 增加 assignment/condition/cast/branch 索引；为
    `type_of_decl_at` / `type_of_member_at` / `type_of_decl_assign_target_at`
    增加正确性守卫下的 O(1) fast path。
  - `RedefinedLocalChecker` 叶子 scope 原地合并，消除每个 `local` 的 parent map clone。
  - `rebuild_dependent_reference_indexes()` 在 SurfaceDelta 为空时直接返回。
  - 新增 `p4_5_tests.rs`（5 个测试）与
    `tools/perf/generate_synthetic_workspace.py`。
  - 实测：`M.x_i = i`×1200 5.50s -> 0.06s；聚合文件 10.92s -> 1.3s；
    全 workspace check 11.41s -> 2.8s。
  - 验证：`cargo test -p emmylua_code_analysis --lib`（1334 passed, 4 ignored）、
    `cargo clippy --workspace --all-targets -- -D warnings`。
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

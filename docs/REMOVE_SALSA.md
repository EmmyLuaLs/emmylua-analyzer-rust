# Remove Salsa Migration Plan

> 目标：彻底移除 Salsa，不再依赖 salsa 的 tracked query / input / memo 体系。
> 原则：每一步保持 `cargo check` 与测试通过；VFS 与 WorkspaceIndex 先脱离 Salsa，最后删除 Salsa 依赖。

## 为什么移除

在“VFS 和 WorkspaceIndex 改为普通数据结构”之后，Salsa 剩余的职责会变得非常薄：

- 不再需要 Salsa 管理文件输入；
- 不再需要 Salsa 管理 workspace shard/index；
- 剩下的 per-node 类型/表达式/flow memo 可以用普通缓存 + 手动失效替代；
- 移除 Salsa 可以降低内存、消除长期单核占用、简化心智模型。

## 迁移总览

```text
当前：
VFS / WorkspaceIndex / SemanticModel / LS / check 全部围绕 SalsaDatabase

目标：
Vfs                普通结构
WorkspaceIndex     普通结构，由 VFS 变更主动维护
SemanticModel      普通结构，持有 Vfs + WorkspaceIndex + 本地缓存
SalsaDatabase      删除
salsa crate        从 Cargo.toml 移除
```

## Phase 0：建立非 Salsa 的核心结构

新建：

```rust
pub struct Vfs {
    files: HashMap<FileId, FileData>,
    next_file_id: u32,
    // line index / path / uri 都放这里
}

pub struct WorkspaceIndex {
    type_index: WorkspaceTypeIndex,
    member_index: WorkspaceMemberIndex,
    decl_index: WorkspaceDeclIndex,
}

pub struct AnalysisState {
    vfs: Vfs,
    workspace: WorkspaceIndex,
}
```

目的：先把 SalsaDatabase 之外的“事实层”独立出来。

## Phase 1：VFS 脱离 Salsa

- [x] `SalsaDatabase` 不再持有 `vfs: Arc<VfsSnapshot>`，改为 `Arc<VfsState>`
- [x] `update_file` / `remove_file` 直接修改普通 `VfsState`
- [x] `SourceFileInput` 与 VFS 分离，只保存在 `file_inputs` 映射中供 Salsa query 使用
- [ ] 后续继续移除 `SourceFileInput` / `WorkspaceInput` 对 Salsa input 的依赖

## Phase 2：FileFacts 脱离 Salsa

- [x] `file_facts` 从 `#[salsa::tracked]` 改为普通函数
- [x] 结果由 `SalsaDatabase.file_facts`（普通 lazy cache）持有
- [x] 文件变化时只更新该文件的 `FileFacts`（config/workspace root 变化时全量重置）
- [ ] 后续将 cache 合并到 `AnalysisState` / `Vfs` 中

## Phase 3：WorkspaceIndex 脱离 Salsa

- [x] `workspace_type_index_for`
- [x] `workspace_member_index_for`
- [x] `workspace_decl_index_for`
- [x] `workspace_module_index_for`
- [x] `workspace_reference_index_for`

全部从 `#[salsa::tracked]` 改为普通函数，结果缓存于 `SalsaDatabase.workspace_index`
（`WorkspaceIndexCache`，普通 `Mutex` + `Arc` 结构）。
`WorkspaceInput.revision` 负责让仍在使用 Salsa tracked query 的上层查询感知 workspace 索引变化。
- [ ] 底层 shard（`export_shard` / `module_shard` / `reference_shard` / `deprecated_shard` / `file_references`）仍由 Salsa 缓存，后续随 Phase 5 一并移除
- [ ] 将 `WorkspaceIndexCache` 合并到 `AnalysisState` / `WorkspaceIndex`

## Phase 4：SemanticModel 脱离 Salsa

> 当前进度：P4a（收敛 `db()` 逃生舱）。
> SemanticModel 内部仍保留 `&SalsaDatabase`，但已开始把常用数据库访问包装成领域方法，
> 并替换 code_analysis 内部的直接 `db()` 调用。

- [x] 为 `SemanticModel` 增加 `document` / `document_current` / `strict_array_index` /
      `file_ids` / `main_workspace_file_ids` / `file_path_of` / `file_uri_of` /
      `module_name_of` / `workspace_id_of` / `is_std_file` / `is_main_file` 等封装
- [x] 替换 `semantic_model/member.rs`、`infer/vm.rs`、checker 内部对 `db()` 的直接使用
- [x] 替换 completion providers 中大量 `model.db()` 用法为 `model_for` / 领域方法
- [x] 清理 `call_hierarchy` / `doc_cli` 的最后两处 `model.db()` 调用
- [x] 全仓库已无实际运行的 `.db()` 调用（仅剩注释）
- [x] 删除 `SemanticModel::db()` 公共逃生舱（内部字段仍保留）
- [ ] `SemanticModel` 不再持有 `&SalsaDatabase`
- [ ] 改为持有：

```rust
pub struct SemanticModel<'a> {
    vfs: &'a Vfs,
    workspace: &'a WorkspaceIndex,
    file_id: FileId,
    cache: RefCell<SemanticLocalCache>,
}
```

- [ ] `self.q()` / `self.db()` 全部替换为直接访问 `Vfs` / `WorkspaceIndex`
- [ ] LS / doc_cli 中通过 `model.db()` 创建其他文件 model 的调用改为 `model.model_for(file_id)`
- [ ] `SalsaSemanticModel` 改名为 `SemanticModel` 或保留别名过渡

## Phase 5：逐模块替换 tracked query

> 状态：`#[salsa::tracked]` 已全部清除（query.rs / exports.rs / flow 均无 tracked query）。

按依赖顺序替换：

1. [x] `resolve_name`（已去掉 `#[salsa::tracked]`，并加入 `SemanticLocalCache.resolve_name`）
2. [ ] `resolve_member`（`SemanticModel` 已有本地缓存；底层 owner/global/module 解析已大量改普通函数）
3. [x] `members_of_owner`（已去掉 `#[salsa::tracked]`，并加入本地缓存）
4. [x] `decl_type`（已去掉 `#[salsa::tracked]`，通过 `expr_type_of` 的 Salsa cycle 收敛）
5. [x] `member_type`（已去掉 `#[salsa::tracked]`，通过 `expr_type_of` 的 Salsa cycle 收敛）
6. [x] `signature_return`（已去掉 `#[salsa::tracked]`，通过 `expr_type_of` 收敛）
7. [x] `param_type`（已去掉 `#[salsa::tracked]`）
8. [x] `expr_type_of`（已去掉 `#[salsa::tracked]`，使用 per-thread in-progress guard 手动防环）
9. [ ] flow / narrowing

同时已将以下非 cycle 查询改为普通函数：
- `resolve_owner` / `resolve_owner_set`
- `global_type_by_name` / `global_decl_by_name` / `type_defs_in_scope`
- `resolve_type_def_locations` / `resolve_type_def`
- `constructor_attribute_of_type`
- `member_keys_of_owner` / `member_keys_of_decl` / `member_keys_of_type`
- `decl_references`
- `file_workspace_id` / `module_file_of`
- `param_type`
- `decl_type` / `member_type`
- `signature_returns` / `signature_return`
- `module_export_type`
- `expr_type_of`（thread-local guard）
- `flow_tree_of`（普通 lazy cache，与 `file_facts` 同步失效）

每个查询改成普通函数 + 手动缓存：

```rust
struct SemanticLocalCache {
    decl_type: HashMap<SemanticId, LuaType>,
    member_type: HashMap<SemanticId, LuaType>,
    expr_type: HashMap<LuaSyntaxId, LuaType>,
    resolve_member: HashMap<LuaSyntaxId, ResolvedMember>,
}
```

失效策略：

- 文件变更时丢弃该文件的局部缓存；
- workspace 索引变更时丢弃跨文件缓存；
- 不需要 salsa 的依赖图/cycle 收敛，改用显式 guard + depth。

## Phase 6：LS / check 入口替换

- `EmmyLuaAnalysis.salsa` 字段删除
- `analysis.semantic_model(file_id)` 改为基于 `AnalysisState`
- `salsa_snapshot()` 删除
- LS 的 `analysis_query` 不再 clone Salsa database

## Phase 7：删除 salsa 依赖

- [x] 删除 Cargo.toml 中 `salsa` 依赖
- [x] 删除 `salsa_builder/query.rs` 中所有 `#[salsa::tracked]`
- [x] 清理所有 `salsa::` 引用
- [ ] 后续可重命名 `salsa_builder` / `SalsaDatabase` / `SalsaSemanticModel` 等兼容名称

## 成功标准

- `cargo check --workspace` 通过
- `cargo test -p emmylua_code_analysis --lib` 通过
- `cargo test -p emmylua_ls --lib` 通过
- `cargo clippy --workspace --all-targets -- -D warnings` 通过
- profile / 性能不比当前差

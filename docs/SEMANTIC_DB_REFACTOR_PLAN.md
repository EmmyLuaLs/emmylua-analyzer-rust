# semantic_db 重构方案（外置读写锁版）

> 状态：设计稿
> 前提：并发由外部读写锁统一负责；数据库内部不引入任何锁或同步状态。

## 0. 外部锁契约

- 写路径必须持有外部写锁，并以 `&mut SemanticDatabase` 独占访问。
- 读路径持有外部读锁，只访问 `&SemanticDatabase`。
- `SemanticModel` 必须被读锁覆盖，不得 `spawn` 到锁外。
- `parallel_for_each_file` 在读锁内每线程一个 model，各自缓存。
- 释放读锁后不得复用旧 model；`revision` 仅调试校验。

- 跨请求缓存放 LSP 层，不放进 DB。
- 禁止 DB 内读时懒加载并写回共享状态。

---

## 1. 硬性禁止清单

- `Mutex` / `RwLock` / `Condvar` / `parking_lot`。
- `OnceLock` / `OnceCell` / `lazy_static`。
- 缓存状态原子变量。
- DB/index 上的 `RefCell` / `Cell` / `UnsafeCell`。
- 读路径修改数据库。

允许：`Arc` 共享不可变数据；写路径在外部写锁下用 `&mut` 更新；`SemanticModel` 请求内缓存由 `&mut self` 显式管理。

---

## 2. 审计摘要

### 2.1 存储重复

一个 member 同时存在于：`FileFacts.members`、`FileExportContribution.members`、raw owner bucket、canonical owner bucket、`by_name`、`all`、`owner_members`、`by_file`。
一个 type 同时存在于：`FileFacts.type_defs`、contribution.types、workspace type bucket、`global_types` 聚合。
reference 同时在 `FileReferences` 和 workspace aggregate 中存 ranges；module / deprecated 也同样多份。

### 2.2 单文件更新放大

- 写路径重建整个 `FileFacts`、`FlowTree`、exports、deprecated、module、`FileReferences`。
- `apply_file_to_workspace_indexes` 遍历所有 workspace，从 5 个索引 remove 再 add。
- 一个 owner 有 K 个 member 时，改 1 个 member 会重建全 owner 的 `all` / `by_name` 列表。
- type 每改一个 key 克隆整桶 `Arc`；reference 对热门 target 做全量 `retain`。
- shard 与 workspace index 重复维护，初始构建每个 workspace 还要扫 64 个 shard。

### 2.3 API 与分层

- `SemanticQueries` facade、`query.rs` free functions、`SemanticModel` 方法形成三套入口。
- 读写函数混在 `query.rs`，`rebuild_*` / `apply_*` 与查询没有类型边界。
- `file_and_config`、`*_input`、`*_for` 保留旧 tracked-query 形状。
- `MemberList` / `TypeDefList` 名义共享 `Arc`，实际 `IntoIterator` 会拷贝。
- `WorkspaceIndexCache` 字段可被外部直接访问，invalidation 靠人工维护。
- 查询路径大量 `expect("index must be built")`。

---

## 3. 目标架构

```text
Vfs / Inputs
  -> FileState (facts + contribution, Arc)
  -> WorkspaceIndex (key -> Bucket<Handle>)
  -> AnalysisView / FileView (read-only)
Mutation: &mut SemanticDatabase + FileDelta
```

原则：符号只存一份；索引只存 handle；派生数据要么写时构建，要么 model-local；读路径不改数据库；单文件更新只处理 delta。

---

## 4. 数据模型

### 4.1 SymbolHandle 与 FileFactsArena

```rust
pub struct SymbolHandle {
    pub file_id: FileId,
    pub index: u32,
}
```

- `FileFacts` 是 `Member` / `TypeDef` / `Decl` 的唯一存储。
- workspace index 只存 `SymbolHandle`，不存 `MemberRef` / `TypeDef` / 字符串副本。
- key 先 intern 成整数 `MemberKeyId` / `OwnerKeyId` / `TypeKeyId`，bucket 不再克隆字符串。

### 4.2 FileContribution 与 OwnerBucket

- `FileContribution` 存 `globals` / `types` / `members` 的 compact handle + key。
- member index 只保留 canonical owner 一套；raw owner 通过 contribution 的映射解析。
- `OwnerBucket` 存 `Vec<SymbolHandle>`，`by_name` 只存位置或 handle，不再复制 `MemberRef`。
- 单 member 修改不重建全 bucket 的派生列表；查询返回 borrowed slice / iterator，需要 snapshot 时显式在 model 内构建。

### 4.3 Reference 与 Derived 状态

- `FileReferences` 只存一份 `(TargetId, TextRange)` 列表；workspace 层存 `by_target` 聚合与 `by_file` key 列表。
- 删除文件只处理该文件 `by_file` 中的 target，不再对热门 target 全量 `retain`。
- `FileState` 中 `flow` / `references` 用 `Derived<T>` 表示：`Ready(Arc<T>)` 或 `Dirty`。
- 写路径只把受影响文件置 `Dirty`；读路径在 `SemanticModel` 内计算并缓存。
- `revision` 是普通 `u64`，写锁下递增，仅用于调试期校验；DB 内无原子变量。

---

## 5. 单文件更新事务

写路径（已持有外部写锁）：

1. 用 `FileDelta` 替换 `FileState.facts` / `contribution`，`flow` / `references` 视变化标记 `Dirty`。
2. 用 old/new contribution 的 keys 更新 workspace index；只访问变化 key 的 bucket，不遍历所有 workspace。
3. 用 `DependencyIndex` 找出 dependents，把它们标记 `Dirty`，不在写路径重建 references。
4. module / deprecated 只在对应 surface 变化时更新。
5. 执行 `revision += 1`。

写路径不做：全量 rebuild index、build flow、重建全文件 references、重建 module shard/index。

---

## 6. API 设计

只读层：

- `AnalysisView`：跨文件查询，持有 `&SemanticDatabase`。
- `FileView`：当前文件查询。
- `SemanticModel`：`FileView + 请求内缓存`，不再到处传 `db + file_id`。

写层：

- `&mut SemanticDatabase::apply_file_change(FileDelta)`。
- `&mut SemanticDatabase::apply_batch(BatchChange)`。
- 读写方法在类型上分离，读 API 不提供 `&mut` 版本。

删除：`SemanticQueries`、`file_and_config`、`*_input` / `*_for` 入口、`MemberList` / `TypeDefList` 假 Arc 包装。

---

## 7. 迁移计划

- Phase A：`AnalysisView` / `FileView` 化，删除 `SemanticQueries`，读写函数分模块，不改存储。
- Phase B：`SymbolHandle` / key interning，contribution 改 compact，workspace index 改 handle bucket。
- Phase C：合并 raw/canonical owner 索引，删除 DB 层 `global_types` / `owner_members` / `owner_members_named` 聚合，reference/module/deprecated 去重。
- Phase D：`Derived<T>` 化 flow/references，写路径只标 `Dirty`，model 内计算缓存。
- Phase E：单文件 `FileDelta` 事务，bucket 只改变化 key；评估删除 shard 层。
- Phase F：尺寸测试、bucket 重写计数、benchmark，清理旧 API。

---

## 8. 验收指标

- 存储：一个 symbol 只存一份 facts，index 中只出现 handle；DB 无跨 workspace 聚合副本。
- 更新：写路径不 build flow / 不重建全文件 references / 不重建全 owner bucket / 不遍历所有 workspace。
- 并发：`semantic_db` 内无任何锁或原子缓存状态；读路径不改 DB。
- API：只读入口 `AnalysisView` / `FileView`，写入入口 `&mut SemanticDatabase`。
- 回归：P0-P10 差分测试、`cargo test --workspace`、clippy 全绿；尺寸与 benchmark 不退化。

---

一句话：把 DB 做成写锁下更新的不可变数据图；读锁下纯查询；symbol 一份，索引存 handle，请求内派生放 model，DB 内部无锁。



---

## 11. 实施进度

- Phase A（进行中）：`SemanticQueries` 已重命名为 `AnalysisView`；`SemanticModel` 已持有 `FileView`；`q()` / query facade 调用已迁移为 `analysis()`；全部测试通过。
- 下一步：把 mutation 从 `query.rs` 拆到独立 update 模块，并逐步把 free functions 收进 `AnalysisView` / `FileView`。
- Phase A 进展：新增 `semantic_db/update.rs`，`WorkspaceFileUpdate` 与 `apply_file_to_workspace_indexes` 已迁入；下一步继续迁移 rebuild/apply 编排函数。
- Phase A 进展：`rebuild_file_after_write` / `rebuild_file_after_remove` 已迁入 `update.rs`，`mod.rs` 写入入口改为调用 `update::*`；下一步迁移 `rebuild_all_caches` 与 index rebuild 编排。
- Phase A 进展：`rebuild_all_caches`、`rebuild_all_module_shards`、`apply_file_reference_index`、`rebuild_module_indexes`、`rebuild_workspace_indexes`、`rebuild_workspace_reference_indexes` 已迁入 `update.rs`，并开放所需 builder 为 `pub(crate)`。
- Phase A 进展：dependency index 与 `refresh_*` 聚合刷新也已迁入 `update.rs`；`AnalysisView::file(file_id) -> FileView` 已加入，`SemanticModel::new` 已改用该入口。
- Phase A 进展：新增 `FileView` 当前文件方法（facts/syntax/decls/members/signatures/name_uses/resolve 等），SemanticModel 内对应调用已从 `.analysis().x(file_id, ...)` 迁移为 `.view.x(...)`。

---

## 12. API 自省（Phase A 后）

- 方向正确：读写模块已分离，`AnalysisView` / `FileView` 已建立，`SemanticQueries` / `q()` 已删除。
- 仍不足：`FileView` 未覆盖全部当前文件查询；`SemanticDatabase` 未收敛为唯一 mutation 入口；`WorkspaceIndex` 字段仍可被直接访问；`MemberList` / `TypeDefList` 仍是假 cheap Arc；查询错误仍以 `expect` / `Option` 为主。
- 结论：当前 API 约 6.5/10。先完成 `FileView` 收口与 `apply_file_change` / `apply_batch`，再进入 Phase B handle 化，避免签名二次返工。

- Phase A 进展：`FileView` 字段已私有化并补齐 accessor；`SemanticModel` 不再直接访问 `view` 字段。
- Phase A 进展：新增统一 mutation API `FileChange` / `BatchChange` / `UpdateSummary`，入口为 `SemanticDatabase::apply_file_change` / `apply_batch`；新增 `p10_apply_file_change_and_batch_api` 回归测试。

## 13. 性能进展

- 已移除 `FileCache.flow` 存储与写路径/全量 rebuild 中的 `build_flow_tree` 调用；`FlowTree` 改为 `SemanticModel` 内的请求级懒缓存（`SemanticLocalCache.flow_trees`）。
- 效果：单文件写入与 batch load 不再为所有文件构建 CFG；未使用 flow 的查询不支付构建成本。
- 约束：仍由外部读锁保护共享 DB，flow 缓存仅属于单次请求的 model，不引入内部锁。

### 13.1 修正：FlowTree 回滚为预构建

- 实测/审查发现每次 `SemanticModel` 创建都会触发 model-local 缓存重建，而每文件会创建大量 model（诊断/遍历/补全），导致 FlowTree 被重复构建。
- 已回滚：`FileCache.flow` 恢复，写路径 / 全量 rebuild 重新预构建 FlowTree；`AnalysisView::flow_tree` / `SemanticDatabase::flow_tree_of`恢复。
- 结论：高频派生结构必须预构建，或放在请求层共享缓存；不能放 model-local 做懒加载。

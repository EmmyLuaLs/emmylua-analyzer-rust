# 重构计划（进行中）

> 目标：在不改变公开 API、不引入行为回归的前提下，把 `emmylua_code_analysis`
> 的巨型模块拆分为按领域组织的目录。每一步都必须通过：
> `cargo fmt --all --check`、`cargo check --workspace --all-targets`、
> `cargo test --workspace`。

## 已完成

### 代码风格
- 移除代码体中的 `crate::...` / `super::...` 全限定调用，统一为顶部 `use` 后使用短名。
- `use super::super::...` 链改为 `crate::...` 绝对路径。
- `macro_rules!` 内部的 `$crate::` 保持不变。

### 测试生命周期
- 移除测试中全部 `Box::leak`（type_check/test.rs、member.rs、infer/test.rs）。
- 测试改为 `TestModel { db, file_id }` + `model()`，模型正常借用测试持有的数据库。

### semantic_db 目录拆分（进行中）
- `index/` 目录已建立，`query.rs` 的 4159 行已开始迁出：
  - `index/types.rs`：WorkspaceTypeIndex、DeprecatedIndex 及构建函数
  - `index/members.rs`：OwnerMembers、WorkspaceMemberIndex 及聚合函数
  - `index/references.rs`：FileReferences、WorkspaceReferenceIndex 及构建函数
- `query.rs` 通过 `pub(crate) use super::index::*::` re-export，外部 `query::X` 路径保持兼容。

## 待做（按顺序）

### 1. 剩余 index 拆分
- `index/decls.rs`：WorkspaceDeclIndex、FileDeclContribution 及构建函数。
- `index/modules.rs`：ModuleEntry、ModuleIndex、ModuleTree、WorkspaceIndexCache。
- 原则：只移动代码和可见性，不改行为；`query::X` 通过 re-export 保持兼容。
- 每迁出一个文件跑一次 check/test，出问题只回滚该批。

### 2. query.rs 按领域拆分
- 目标：`query/{types,members,modules,globals,exports,references,flow}.rs`。
- 纯查询与类型求值函数按领域移动；跨模块符号提升为 `pub(crate)`。
- 最终 `query/mod.rs` 只做 re-export 和入口，不再承载具体实现。

## 多线程优化（进行中）

### 问题
- CPU 密集分析直接跑在 tokio worker 上，workspace 诊断容易饿死 runtime。
- run_workspace_batch 每文件一个 tokio::spawn + Semaphore(64)，并发无全局上界。
- std::sync::RwLock 全局锁：长查询拖住 didChange，写锁又反压所有请求。
- 语义取消用 panic 穿过任务边界，靠嵌套 spawn 兜底。
- 通知异步分支 spawn 后不 await，open/change/close 入队顺序不严格。

### 目标模型
- async task 不跑 CPU 重活，统一进入有界 blocking 池。
- 分析并发上界 = `available_parallelism()`，通过 Semaphore 控制。
- 写更新不阻塞 runtime worker，单写者队列保持顺序。
- 通知按 LSP 顺序串行进入 update queue。
- 取消改为协作式 Result，移除嵌套 spawn + panic 控制流。

### 已完成（Phase 1）
- AnalysisState：Arc<RwLock> + Semaphore，新增 run_blocking / query_blocking。
- update 改为 block_in_place + permit，避免 didChange 卡 async worker。
- query_runner 两个入口改走 query_blocking，覆盖 61 个 LSP 请求 handler。
- DiagnosticService：单文件与 workspace 诊断改走 run_blocking，移除 Semaphore(64)。
- notification_handler 异步通知 spawn 后 await，保证 open/change/close 入队顺序。

### 后续（Phase 2+）
- 快照模型：用 Arc snapshot 替换全局 RwLock，读请求无锁快照。
- 诊断调度：固定 N 个 worker 从文件队列取任务，替代每文件 spawn。
- 取消语义：改为 Result 传播，移除嵌套 spawn 与 panic 驱动取消。
- 可观测性：增加 blocking 队列、permit 等待、RwLock 等待指标。
- 压测：大 workspace 诊断 + 高频 completion + 连续 didChange/取消。

### 验收标准
- cargo fmt --all --check
- cargo check --workspace --all-targets
- cargo test --workspace
- 公开 API 不变；query::X / semantic_db 旧路径保持兼容。

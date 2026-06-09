# Salsa API 确定性设计总结 & Phase 3 迁移计划

> 基于 `compilation_semantic_architecture_CN.md` 和当前代码（branch: `salsa2`，commit `9fdb4e38`）的审核结果。

---

## 第一部分：下一阶段（Phase 3）该做什么

### 当前状态速览

- **Phase 1（alias/origin/class/enum 域）** ✅ 完成 — 15 个文件，30+ 处迁移
- **Phase 2（Salsa 基础设施 + Public API 收口）** ✅ 完成 — analyzer 责任清单、跨文件查询基础设施（type_def_reverse_index, SummaryFileListInput）、salsa_db/summary → pub(crate)、lib.rs 分层
- **Semantic 层剩余遗留**: **153 处** legacy index 直接访问，分布在 **39 个文件**

### Phase 3 的核心命题

**不再是"证明 summary-first 可行"，而是系统性地消除 `semantic/` 下 153 处 legacy index 读。**

这 153 处分布在以下热点（按收益排序）：

| 热点领域 | 文件 | 访问量 | 迁移难度 | 关键依赖 |
|---|---|---|---|---|
| **infer_index** | `infer_index/mod.rs` | 17 | 中 | type_index, member_index, decl_index, signature_index, operator_index |
| **infer_call** | `infer_call/mod.rs` | 11 | 高 | operator_index（无 Salsa 替代） |
| **find_members** | `find_members.rs` | 9 | 中 | member_index, type_index |
| **find_index** | `find_index.rs` | 8 | 中 | member_index, type_index, signature_index |
| **semantic_info** | `semantic_info/mod.rs` + `infer_expr_semantic_decl.rs` | 14 | 高 | type_index, decl_index, member_index |
| **ref_type** | `ref_type.rs` | 7 | 中 | type_index, signature_index |
| **infer_name** | `infer_name.rs` | 6 | 中 | decl_index |
| **infer_table** | `infer_table.rs` | 6 | 中 | type_index, decl_index |
| **instantiate_type** | 4 文件 | 11 | 高 | type_index, signature_index |
| **type_check** | 8 文件 | 24 | 中 | type_index, signature_index, member_index |
| **visibility** | `visibility/mod.rs` | 4 | 低 | member_index, type_index |

### Phase 3 分步执行建议

#### Phase 3a — 建立缺失的 Salsa 查询基元（2-3 天）

当前投影层无法落地的最核心 blocker：**Salsa 层缺少对以下 legacy index 的替代查询**。

**P3a.1 `SalsaSummaryTypeQueries` 补充 operator 查询**

当前 `operator_index` 没有任何 Salsa 对应。这 block 了 `infer_call/mod.rs`（11 处）的全部迁移。

需要新增：
```rust
// tracked 函数
#[salsa::tracked]
fn file_operator_summary(db: &dyn SummaryDb, file: SummarySourceFileInput, config: SummaryConfigInput)
    -> Option<Arc<SalsaOperatorIndexSummary>>;

// facade 方法
pub fn operators(&self, file_id: FileId) -> Option<Arc<SalsaOperatorIndexSummary>>;
```

**P3a.2 `SalsaSummaryTypeQueries` 补充 type_cache 等价查询**

当前 `type_index.get_type_cache()` 是最大的单一依赖（~40 处）。SemanticModel 的 `get_type()` 方法直接调用 `self.db.get_type_index().get_type_cache()`。

需要在 Salsa 层建立类型运算的 projection query：
```rust
// tracked 函数
#[salsa::tracked]
fn file_type_cache(db: &dyn SummaryDb, file: SummarySourceFileInput, config: SummaryConfigInput,
    type_owner: SalsaTypeOwner) -> Option<SalsaTypeCacheSummary>;
```

**P3a.3 建立 member dispatch 投影（替代 member_index 读）**

`find_members.rs` / `find_index.rs`（17 处）的核心访问模式是 `member_index.get_member_item()`。

需要新增：
```rust
// 在 compilation/member.rs 中新增
pub(crate) fn get_member_dispatch(db: &DbIndex, member_key: &LuaMemberKey, prefix_type: &LuaType) 
    -> Option<SalsaMemberDispatchSummary>;
```

#### Phase 3b — 逐领域迁移（按热点顺序）

**顺序原则**：先处理可以独立完成的领域，operator 和 member_index 依赖 P3a 完成后再做。

**B1：`infer_index/mod.rs`（17 处）** — 这是单个文件中最多的遗留引用。

当前状态：9 处 alias/origin/class/enum/super_types 已在 Phase 1 完成迁移。剩余主要是：
- `type_index.get_type_cache()` — 依赖 P3a.2
- `member_index.get_member_item()` — 依赖 P3a.3
- `signature_index` — 需要 signature query projection
- `operator_index` — 依赖 P3a.1
- 1 处 `is_enum_key()` 已保留 legacy fallback

**B2：`find_members.rs` + `find_index.rs`（17 处）**

当前状态：alias/origin 已迁移到 summary-backed 投影。剩余：
- `member_index.get_member_item()` → 依赖 P3a.3
- `get_type_index().get_type_cache()` → 依赖 P3a.2
- `get_type_index().get_super_types()` → 需要 super type query

**B3：`type_check/` 组（24 处，8 文件）**

大部分 alias/class/enum 已在 Phase 1 迁移。剩余核心依赖：
- `type_index.get_type_cache()` → P3a.2
- `type_index.get_super_types()` → super type query
- 类型运算（`TypeOps`）内部回退到 legacy index

**B4：`instantiate_type/` 组（11 处，4 文件）**

泛型实例化需要 type_index 获取泛型参数信息。Phase 1 已迁移 alias/origin，但核心泛型参数查询仍走 legacy。

#### Phase 3c — 搬走 `SemanticModel.get_db()` 和 `get_type()`

`SemanticModel` 暴露了 `get_db(&self) -> &DbIndex` 和 `get_type()` 直接读 `type_index`。这是外部使用 legacy index 的最主要入口。

步骤：
1. 将 `get_type()` 改为使用 Salsa projection（依赖 P3a.2）
2. 逐个检查 100+ 处 external `.get_db()` 调用，为每个调用添加 facade 方法
3. 最后将 `get_db()` 从 `pub` 改为 `pub(crate)`

#### Phase 3d — `infer_call/mod.rs`（11 处，最困难的迁移）

这是最大单一遗留块，核心原因是 operator dispatch 没有 Salsa 替代。

```
semantic/infer/infer_call/mod.rs → operator_index → 无 Salsa 对应
                                  → signature_index → 有部分 Salsa 替代
                                  → type_index → 部分替代
```

**必须先完成 P3a.1**（operator 查询）才能开始这个文件。

### Phase 3 完成的定义

当以下条件满足时，Phase 3 结束：

1. `semantic/` 下 153 处 legacy index 读 → 降到 **30 以内**（可枚举的残余）
2. `type_index.get_type_cache()` 的 consumer 已经切换到 Salsa-backed projection
3. `operator_index` 和 `member_index.get_member_item()` 读路径已有 Salsa 替代
4. `SemanticModel.get_db()` → `pub(crate)`（或者至少它的 hot path 不再需要外部使用者绕过来访问 legacy index）
5. 关键测试（type_check, infer, member, generic）全部通过

### Phase 4（之后）— 删除 analyzer 目录 + 收窄 pub use db_index::*

这个只在 Phase 3 完成后才可行。届时：
- `db_index` 可以退化为 `SalsaSummaryDatabase + Vfs + DiagnosticIndex + JsonSchemaIndex` 薄壳
- `pub use db_index::*` 可以大幅收缩
- analyzer 目录可以安全删除

---

## 第二部分：Salsa API 确定性设计总结

### 1. 整体架构

```
┌─────────────────────────────────────────────────────────┐
│  External Consumers (emmylua_ls / emmylua_check / doc)   │
│  via EmmyLuaAnalysis / LuaCompilation / SemanticModel    │
├─────────────────────────────────────────────────────────┤
│  compilation (projection layer)                          │
│  member.rs / decl.rs / module.rs / global.rs / resolve.rs│
├─────────────────────────────────────────────────────────┤
│  summary_builder.query (index/query layer)               │
│  decl_member / doc_type / doc_tag / flow / lexical /     │
│  module / signature / semantic / semantic_graph /        │
│  semantic_solver / type_system / table_shape             │
├─────────────────────────────────────────────────────────┤
│  summary_builder.salsa_db.tracked (131 tracked functions) │
│  mod.rs (86) + doc.rs (21) + lexical.rs (24)             │
├─────────────────────────────────────────────────────────┤
│  summary_builder.salsa_db.facade (7 query facades)       │
│  File / Doc / Lexical / Flow / Module / Semantic / Types │
├─────────────────────────────────────────────────────────┤
│  summary_builder.analysis (14 file-local analyzers)      │
│  decl / doc / doc_type / file / flow / module /          │
│  module_returns / owner_binding / property / signature / │
│  support / table_shape / use_site                        │
├─────────────────────────────────────────────────────────┤
│  Salsa Inputs (4)                                        │
│  SummarySourceFileInput / SummaryWorkspaceInput /        │
│  SummaryConfigInput / SummaryFileListInput               │
├─────────────────────────────────────────────────────────┤
│  SalsaSummaryDatabase (salsa::Storage + files + config)  │
└─────────────────────────────────────────────────────────┘
```

### 2. 数据流

```
文件变更
  │
  ▼
DbIndex::sync_summary_file(file_id)
  │
  ├─► vfs.get_file_content() → text snapshot
  ├─► SalsaSummaryDatabase::set_file(file_id, path, text, is_remote)
  │     │
  │     └─► 创建 SummarySourceFileInput（#[salsa::input]）
  │           自动触发受影响的 tracked 函数失效
  │
  └─► 递增 type_def_rev_gen（反向索引缓存失效）
        │
        ▼
   下次查询时惰性重建 TypeDefReverseIndex
```

### 3. Input 层（4 个 Salsa Input）

| Input | 声明位置 | 字段 | 用途 |
|---|---|---|---|
| `SummarySourceFileInput` | `inputs/mod.rs:94` | `file_id, path, text, is_remote` | 单个文件的不可变快照 |
| `SummaryWorkspaceInput` | `inputs/mod.rs:102` | `workspaces: Vec<Workspace>` | workspace 配置快照 |
| `SummaryConfigInput` | `inputs/mod.rs:107` | `config: SalsaSummaryConfig` | Emmyrc 派生配置快照 |
| `SummaryFileListInput` | `inputs/mod.rs:114` | `file_ids: Vec<FileId>` | **跨文件查询依赖追踪**。每次增删文件时更新 |

**关键设计决策**：所有 input 都是通过替换（而非修改）来更新。`SalsaSummaryDatabase.set_file()` 创建新的 `SummarySourceFileInput` 并替换 HashMap 中的旧值。Salsa 自动检测 input 变化并失效下游 tracked 函数。

### 4. Database 层

#### 核心类型

```rust
#[salsa::db]
pub trait SummaryDb: salsa::Database {}

#[salsa::db]
pub struct SalsaSummaryDatabase {
    storage: salsa::Storage<Self>,
    files: HashMap<FileId, SummarySourceFileInput>,
    workspaces: Option<SummaryWorkspaceInput>,
    config: Option<SummaryConfigInput>,
    file_list: Option<SummaryFileListInput>,
}
```

#### 生命周期方法

```rust
impl SalsaSummaryDatabase {
    // 配置
    pub fn update_config(&mut self, emmyrc: Arc<Emmyrc>);
    pub fn set_workspaces(&mut self, workspaces: Vec<Workspace>);

    // 文件管理（会触发 Salsa 失效 + 更新 file_list）
    pub fn set_file(&mut self, file_id: FileId, path, text, is_remote);
    pub fn set_file_from_vfs(&mut self, vfs: &Vfs, file_id: FileId) -> bool;
    pub fn remove_file(&mut self, file_id: FileId);
    pub fn clear(&mut self);

    // 查询入口（返回 7 个 facade）
    pub fn file(&self) -> SalsaSummaryFileQueries<'_>;
    pub fn doc(&self) -> SalsaSummaryDocQueries<'_>;
    pub fn lexical(&self) -> SalsaSummaryLexicalQueries<'_>;
    pub fn flow(&self) -> SalsaSummaryFlowQueries<'_>;
    pub fn module(&self) -> SalsaSummaryModuleQueries<'_>;
    pub fn semantic(&self) -> SalsaSummarySemanticQueries<'_>;
    pub fn types(&self) -> SalsaSummaryTypeQueries<'_>;

    // 跨文件迭代
    pub(crate) fn file_ids(&self) -> Vec<FileId>;
}
```

#### 包装类型

```rust
pub struct SalsaSummaryHost {
    db: SalsaSummaryDatabase,
    vfs: Vfs,  // 独立 VFS，与 DbIndex.vfs 可能不同步（测试用）
}
```

`SalsaSummaryHost` 主要在测试中使用。生产路径中，`DbIndex` 持有 `RwLock<SalsaSummaryDatabase>` 并通过 `get_summary_db()` 访问。

### 5. 7 个 Query Facade 完整 API 目录

所有 facade 方法都接受 `file_id: FileId` 作为第一个参数（per-file 查询模型）。

#### 5.1 `SalsaSummaryFileQueries` — 文件基础查询（20 方法）

```rust
// 文件级摘要
pub fn summary(&self, file_id: FileId) -> Option<Arc<SalsaFileSummary>>;

// 声明树
pub fn decl_tree(&self, file_id: FileId) -> Option<Arc<SalsaDeclTreeSummary>>;
pub fn decl_by_syntax_id(&self, file_id, syntax_id) -> Option<SalsaDeclSummary>;

// 全局变量/函数
pub fn globals(&self, file_id: FileId) -> Option<Arc<SalsaGlobalSummary>>;

// 成员索引
pub fn members(&self, file_id: FileId) -> Option<Arc<SalsaMemberIndexSummary>>;
pub fn member_by_syntax_id(&self, file_id, syntax_id) -> Option<SalsaMemberSummary>;

// 属性（@field / @operator 等）
pub fn properties(&self, file_id: FileId) -> Option<Arc<SalsaPropertyIndexSummary>>;
pub fn property_at(&self, file_id, syntax_offset) -> Option<SalsaPropertySummary>;
pub fn properties_for_decl/member/key/type/source(...) -> Option<Vec<...>>;
//   (12 个变体方法用于按不同维度查询属性)

// Table shape（对象形状定义）
pub fn table_shapes(&self, file_id: FileId) -> Option<Arc<SalsaTableShapeIndexSummary>>;
pub fn table_shape_at(&self, file_id, syntax_offset) -> Option<SalsaTableShapeSummary>;
```

#### 5.2 `SalsaSummaryDocQueries` — Doc 注释查询（18 方法）

```rust
// 获取子 facade
pub fn signature(&self) -> SalsaSummaryDocSignatureQueries<'_>;

// Doc 摘要
pub fn summary(&self, file_id) -> Option<Arc<SalsaDocSummary>>;

// Tag 查询（@class, @enum, @alias, @field, @operator, @param, @return 等）
pub fn tags(&self, file_id) -> Option<Vec<SalsaDocTagSummary>>;
pub fn tag_at(&self, file_id, syntax_offset) -> Option<SalsaDocTagSummary>;
pub fn tags_for_kind(&self, file_id, kind) -> Option<Vec<SalsaDocTagSummary>>;
pub fn tags_for_owner(&self, file_id, owner) -> Option<Vec<SalsaDocTagSummary>>;

// Tag 属性（@field 的字段标签等）
pub fn tag_properties(&self, file_id) -> Option<Vec<SalsaDocTagPropertySummary>>;
pub fn tag_property(&self, file_id, owner) -> Option<SalsaDocTagPropertySummary>;

// 诊断
pub fn resolved_tag_diagnostics(&self, file_id, owner) -> Option<Vec<SalsaResolvedDocDiagnosticActionSummary>>;

// 类型定义（@class, @enum, @alias 定义的 doc type）
pub fn types(&self, file_id) -> Option<Arc<SalsaDocTypeIndexSummary>>;
pub fn lowered_types(&self, file_id) -> Option<Arc<SalsaDocTypeLoweredIndex>>;
pub fn type_def_index(&self, file_id) -> Option<Arc<SalsaDocTypeDefQueryIndex>>;
pub fn type_def_by_name(&self, file_id, name: SmolStr) -> Option<SalsaDocTypeDefSummary>;
pub fn lowered_type_at/by_key(&self, file_id, offset/key) -> Option<SalsaDocTypeLoweredNode>;
pub fn resolved_types(&self, file_id) -> Option<Arc<SalsaDocTypeResolvedIndex>>;
pub fn resolved_type_at/by_key(&self, file_id, offset/key) -> Option<SalsaDocTypeResolvedSummary>;

// Owner 解析（doc 标签的归属解析）
pub fn owner_bindings(&self, file_id) -> Option<Arc<SalsaDocOwnerBindingIndexSummary>>;
pub fn owner_resolve_index(&self, file_id) -> Option<Arc<SalsaDocOwnerResolveIndex>>;
pub fn owner_resolve(&self, file_id, owner_offset) -> Option<SalsaDocOwnerResolveSummary>;
```

**子 facade: `SalsaSummaryDocSignatureQueries`**（7 方法）

```rust
pub fn summary(&self, file_id) -> Option<Arc<SalsaSignatureIndexSummary>>;
pub fn explain_index(&self, file_id) -> Option<Arc<SalsaSignatureExplainIndex>>;
pub fn explain(&self, file_id, signature_offset) -> Option<SalsaSignatureExplainSummary>;
pub fn generic_param(&self, file_id, owner_offset, name) -> Option<SalsaSignatureGenericParamLookupSummary>;
pub fn call_explain(&self, file_id, call_offset) -> Option<SalsaCallExplainSummary>;
pub fn return_index(&self, file_id) -> Option<Arc<SalsaSignatureReturnQueryIndex>>;
pub fn return_query(&self, file_id, signature_offset) -> Option<SalsaSignatureReturnQuerySummary>;
```

#### 5.3 `SalsaSummaryLexicalQueries` — 词法/引用查询（15 方法）

```rust
// Use sites（符号使用点）
pub fn use_sites(&self, file_id) -> Option<Arc<SalsaUseSiteIndexSummary>>;
pub fn use_index(&self, file_id) -> Option<Arc<SalsaLexicalUseIndex>>;
pub fn use_at(&self, file_id, syntax_offset) -> Option<SalsaLexicalUseSummary>;

// 名称解析
pub fn name_resolution(&self, file_id, syntax_offset) -> Option<SalsaNameUseSummary>;
pub fn name_resolution_by_syntax_id(&self, file_id, syntax_id) -> Option<SalsaNameUseSummary>;

// 成员解析
pub fn member_resolution(&self, file_id, syntax_offset) -> Option<SalsaMemberUseSummary>;
pub fn member_resolution_by_syntax_id(&self, file_id, syntax_id) -> Option<SalsaMemberUseSummary>;

// 调用解析
pub fn call_at(&self, file_id, syntax_offset) -> Option<SalsaCallUseSummary>;
pub fn call_at_by_syntax_id(&self, file_id, syntax_id) -> Option<SalsaCallUseSummary>;

// 反向引用查询（decl → 所有引用它的 use site）
pub fn decl_references(&self, file_id, decl_id) -> Option<Vec<SalsaNameUseSummary>>;
pub fn global_name_references(&self, file_id, name) -> Option<Vec<SalsaNameUseSummary>>;
pub fn member_references(&self, file_id, member_target) -> Option<Vec<SalsaMemberUseSummary>>;
pub fn call_references_for_name(&self, file_id, callee_name) -> Option<Vec<SalsaCallUseSummary>>;
```

#### 5.4 `SalsaSummaryFlowQueries` — 控制流查询（22 方法）

```rust
pub fn summary(&self, file_id) -> Option<Arc<SalsaFlowSummary>>;

// 按位置查询流节点
pub fn block_at/branch_at/loop_at/return_at/break_at/goto_at/condition/label(...);

// 流图查询
pub fn query(&self, file_id) -> Option<Arc<SalsaFlowQuerySummary>>;
pub fn successors/predecessors(&self, file_id, node) -> Option<Vec<SalsaFlowNodeRefSummary>>;
pub fn outgoing_edges/incoming_edges(&self, file_id, node) -> Option<Vec<SalsaFlowEdgeSummary>>;
pub fn reachable_nodes(&self, file_id, start) -> Option<Vec<SalsaFlowNodeRefSummary>>;
pub fn can_reach(&self, file_id, from, to) -> Option<bool>;

// 子图查询
pub fn condition_graph/branch_graph/loop_graph/return_graph/break_graph/goto_graph(...);

// For-range 迭代
pub fn for_range_iter_index(&self, file_id) -> Option<Arc<SalsaForRangeIterQueryIndex>>;
pub fn for_range_iter(&self, file_id, loop_offset) -> Option<SalsaForRangeIterQuerySummary>;
```

#### 5.5 `SalsaSummaryModuleQueries` — 模块查询（13 方法）

```rust
pub fn resolve_index(&self, file_id) -> Option<Arc<SalsaModuleResolveIndex>>;
pub fn summary(&self, file_id) -> Option<Arc<SalsaModuleSummary>>;

// 导出相关
pub fn export_target(&self, file_id) -> Option<SalsaExportTargetSummary>;
pub fn export(&self, file_id) -> Option<SalsaModuleExportSummary>;
pub fn exported_global_function/variable(&self, file_id) -> Option<SalsaGlobalFunction/VariableSummary>;

// 导出判断（按不同目标类型）
pub fn exports_decl(&self, file_id, decl_id) -> Option<bool>;
pub fn exports_member(&self, file_id, member_target) -> Option<bool>;
pub fn exports_closure/table/global_function/global_variable(...) -> Option<bool>;
```

#### 5.6 `SalsaSummarySemanticQueries` — 语义查询（2 个子 facade）

```rust
pub fn file(&self) -> SalsaSummarySemanticFileQueries<'_>;
pub fn target(&self) -> SalsaSummarySemanticTargetQueries<'_>;
```

**子 facade: `SalsaSummarySemanticFileQueries`**（33 方法）

```rust
// 单文件语义摘要
pub fn summary(&self, file_id) -> Option<Arc<SalsaSingleFileSemanticSummary>>;
pub fn tag_properties/required_modules/module_export/module_export_query(...);

// 语义图（SCC 分解）
pub fn graph(&self, file_id) -> Option<Arc<SalsaSemanticGraphSummary>>;
pub fn graph_index(&self, file_id) -> Option<Arc<SalsaSemanticGraphQueryIndex>>;
pub fn graph_scc_index/component/successors/predecessors(...);

// 语义求解器（fixedpoint 执行）
pub fn solver_worklist/execution/execution_task/step/task/ready_tasks(...);
pub fn solver_execution_is_complete(&self, file_id) -> Option<bool>;

// Value shell（用于声明推断）
pub fn decl_value_shell/member_value_shell/signature_return_value_shell(...);
pub fn module_export_value_shell/for_range_iter_value_shell(...);

// 类型查询结果
pub fn decl_component_result_summary/member_component_result_summary(...);
pub fn decl_summary/member_summary(...);
```

**子 facade: `SalsaSummarySemanticTargetQueries`**（5 方法）

```rust
pub fn index(&self, file_id) -> Option<Arc<SalsaSemanticTargetQueryIndex>>;
pub fn decl(&self, file_id, decl_id) -> Option<SalsaSemanticTargetInfoSummary>;
pub fn member(&self, file_id, member_target) -> Option<SalsaSemanticTargetInfoSummary>;
pub fn signature(&self, file_id, signature_offset) -> Option<SalsaSemanticTargetInfoSummary>;
pub fn signature_explain(&self, file_id, signature_offset) -> Option<SalsaSignatureExplainSummary>;
```

#### 5.7 `SalsaSummaryTypeQueries` — 类型查询（11 方法）

```rust
// 声明类型
pub fn decl_index(&self, file_id) -> Option<Arc<SalsaDeclTypeQueryIndex>>;
pub fn decl(&self, file_id, decl_id) -> Option<SalsaDeclTypeInfoSummary>;

// 全局类型
pub fn global_index(&self, file_id) -> Option<Arc<SalsaGlobalTypeQueryIndex>>;
pub fn global(&self, file_id, name) -> Option<SalsaGlobalTypeInfoSummary>;
pub fn global_name(&self, file_id, syntax_offset) -> Option<SalsaGlobalTypeInfoSummary>;

// 成员类型
pub fn member_index(&self, file_id) -> Option<Arc<SalsaMemberTypeQueryIndex>>;
pub fn member(&self, file_id, member_target) -> Option<SalsaMemberTypeInfoSummary>;
pub fn member_use(&self, file_id, syntax_offset) -> Option<SalsaMemberTypeInfoSummary>;
pub fn member_at(&self, file_id, syntax_offset, program_point) -> Option<SalsaProgramPointMemberTypeInfoSummary>;

// 名称类型（program point 敏感）
pub fn name(&self, file_id, syntax_offset) -> Option<SalsaNameTypeInfoSummary>;
pub fn name_at(&self, file_id, syntax_offset, program_point) -> Option<SalsaProgramPointTypeInfoSummary>;
```

### 6. Projection 层 API（compilation/）

这是 facade 和 semantic consumer 之间的适配层。所有函数都是普通 Rust 函数（非 `#[salsa::tracked]`），接受 `&DbIndex`。

#### `compilation::member`（pub(crate)，5 函数）

```rust
pub(crate) fn get_type_def_kind(db: &DbIndex, type_decl_id: &LuaTypeDeclId) 
    -> Option<SalsaDocTypeDefKindSummary>;
pub(crate) fn type_def_is_class(db: &DbIndex, type_decl_id: &LuaTypeDeclId) -> bool;
pub(crate) fn type_def_is_alias(db: &DbIndex, type_decl_id: &LuaTypeDeclId) -> bool;
pub(crate) fn type_def_is_enum(db: &DbIndex, type_decl_id: &LuaTypeDeclId) -> bool;
pub(crate) fn type_def_alias_origin(db: &DbIndex, type_decl_id: &LuaTypeDeclId) -> Option<LuaType>;
```

> 已通过 `TypeDefReverseIndex` 实现 O(1) 查找（替代 O(files) 扫描）。

#### `compilation::decl`（pub，主要函数）

```rust
// 类型定义信息
pub struct CompilationGenericParamInfo { name, constraint, default_type }
pub struct CompilationDeclInfo { file_id, decl_id, summary, decl_type }

// 查找声明
pub fn find_compilation_decl_by_position(db, file_id, position) -> Option<CompilationDeclInfo>;
pub fn find_compilation_decl_by_syntax_id(db, file_id, syntax_id) -> Option<CompilationDeclInfo>;

// 泛型参数
pub fn find_compilation_param_generic_params(db, decl) -> Option<Vec<CompilationGenericParamInfo>>;
pub fn find_compilation_type_generic_params(db, type_decl_id) -> Option<Vec<CompilationGenericParamInfo>>;

// 类型推断投影
pub(crate) fn infer_compilation_doc_type_key_with_owner(db, file_id, owner, type_key) -> Option<LuaType>;
// ... 更多内部辅助函数
```

#### `compilation::module`（pub）

```rust
pub struct CompilationModuleInfo { file_id, full_module_name, name, visible, workspace_id, 
                                    is_meta, export_target, export, semantic_target }

pub(crate) fn project_module_info(db, file_id) -> Option<CompilationModuleInfo>;
pub(crate) fn find_module_by_require_path(db, module_path) -> Option<CompilationModuleInfo>;
pub fn resolve_projected_module_export_type(db, file_id) -> Option<LuaType>;
```

#### `compilation::global`（pub(crate)）

```rust
pub struct CompilationGlobals { decls, functions }
pub fn globals(db, name) -> CompilationGlobals;
pub fn global_type(db, name) -> Option<LuaType>;
```

#### `compilation::resolve`（pub(crate)）

```rust
// 跨文件依赖拓扑排序
pub(crate) fn get_cross_file_resolve_order(db: &DbIndex) -> Vec<FileId>;
```

### 7. 现有 facade（消费者主入口）

#### `LuaCompilation`（pub）

```rust
impl LuaCompilation {
    pub fn new(emmyrc) -> Self;
    pub fn get_semantic_model(&self, file_id) -> Option<SemanticModel<'_>>;

    // 模块查询
    pub fn find_module_by_file_id(&self, file_id) -> Option<CompilationModuleInfo>;
    pub fn find_module_by_require_path(&self, module_path) -> Option<CompilationModuleInfo>;
    pub fn resolve_module_export_type(&self, file_id) -> Option<LuaType>;

    // 泛型参数
    pub fn find_type_generic_params(&self, type_decl_id) -> Option<Vec<CompilationGenericParamInfo>>;

    // 索引生命周期
    pub fn update_index(&mut self, file_ids);
    pub fn remove_index(&mut self, file_ids);
    pub fn clear_index(&mut self);
    pub fn update_config(&mut self, config);

    // ⚠️ 泄漏了 DbIndex（应该最终改为 pub(crate)）
    pub fn get_db(&self) -> &DbIndex;
    pub fn get_db_mut(&mut self) -> &mut DbIndex;
}
```

#### `SemanticModel`（pub）

```rust
impl SemanticModel {
    // 模块
    pub fn get_module(&self) -> Option<CompilationModuleInfo>;
    pub fn find_module_by_require_path(&self, module_path) -> Option<CompilationModuleInfo>;

    // 类型推断
    pub fn infer_expr(&self, expr) -> Result<LuaType, InferFailReason>;
    pub fn infer_table_should_be(&self, table) -> Option<LuaType>;
    pub fn infer_call_expr_func(&self, call_expr, arg_count) -> Option<Arc<LuaFunctionType>>;
    pub fn infer_bind_value_type(&self, expr) -> Option<LuaType>;
    pub fn infer_member_type(&self, prefix_type, member_key) -> Result<LuaType, InferFailReason>;

    // 成员查询
    pub fn get_member_infos(&self, prefix_type) -> Option<Vec<LuaMemberInfo>>;
    pub fn get_member_info_with_key(&self, prefix_type, key, find_all) -> Option<Vec<LuaMemberInfo>>;
    pub fn get_member_info_map(&self, prefix_type) -> Option<HashMap<LuaMemberKey, Vec<LuaMemberInfo>>>;
    pub fn get_member_origin_owner(&self, member_id) -> Option<LuaSemanticDeclId>;

    // 类型检查和语义
    pub fn type_check(&self, source, compact_type) -> TypeCheckResult;
    pub fn type_check_detail(&self, source, compact_type) -> TypeCheckResult;
    pub fn is_sub_type_of(&self, sub_type_id, super_type_id) -> bool;
    pub fn is_reference_to(&self, node, semantic_decl_id, level) -> bool;
    pub fn is_semantic_visible(&self, token, property_owner) -> bool;

    // 语义信息
    pub fn get_semantic_info(&self, node_or_token) -> Option<SemanticInfo>;
    pub fn find_decl(&self, node_or_token, level) -> Option<LuaSemanticDeclId>;
    pub fn get_type(&self, type_owner) -> LuaType;             // ⚠️ 直接读 type_index
    pub fn get_index_decl_type(&self, index_expr) -> Option<LuaType>;

    // VFS
    pub fn get_document(&self) -> LuaDocument<'_>;
    pub fn get_root(&self) -> &LuaChunk;
    pub fn get_file_id(&self) -> FileId;

    // ⚠️ 泄漏了 DbIndex
    pub fn get_db(&self) -> &DbIndex;
    pub fn get_emmyrc(&self) -> &Emmyrc;
}
```

#### `EmmyLuaAnalysis`（pub，最顶层入口）

```rust
impl EmmyLuaAnalysis {
    pub fn new() -> Self;
    pub fn init_std_lib(&mut self, create_resources_dir);

    // 文件管理
    pub fn update_file_by_uri(&mut self, uri, text) -> Option<FileId>;
    pub fn update_files_by_uri(&mut self, files) -> Vec<FileId>;
    pub fn update_file_by_path(&mut self, path, text) -> Option<FileId>;
    pub fn remove_file_by_uri(&mut self, uri) -> Option<FileId>;
    pub fn reload_workspace_files(&mut self, files, open_files) -> Vec<Uri>;

    // Workspace 管理
    pub fn add_main_workspace(&mut self, root);
    pub fn add_library_workspace(&mut self, workspace);
    pub fn clear_non_std_workspaces(&mut self);

    // 配置 & 诊断
    pub fn update_config(&mut self, config);
    pub fn diagnose_file(&self, file_id, cancel_token) -> Option<Vec<lsp_types::Diagnostic>>;

    // URI/FileId 转换
    pub fn get_file_id(&self, uri) -> Option<FileId>;
    pub fn get_uri(&self, file_id) -> Option<Uri>;
}
```

### 8. 使用模式

#### 模式 1：从文件句柄查询类型定义

```rust
// 获取文件的 doc 中定义的类型
let summary_db = db.get_summary_db();

// 查询文件的全部类型定义
if let Some(type_index) = summary_db.doc().types(file_id) {
    for type_def in &type_index.entries {
        println!("type: {:?}", type_def.name);
    }
}

// 按名称精确查找
if let Some(type_def) = summary_db.doc().type_def_by_name(file_id, "MyClass".into()) {
    match type_def.kind {
        SalsaDocTypeDefKindSummary::Class => { /* ... */ }
        SalsaDocTypeDefKindSummary::Alias => { /* ... */ }
        SalsaDocTypeDefKindSummary::Enum => { /* ... */ }
        _ => {}
    }
}
```

#### 模式 2：跨文件类型反向查找（通过 projection 层）

```rust
// 使用 TypeDefReverseIndex 跨文件查找
// 不要直接循环 file_ids + 逐文件 type_def_by_name
// 这样写：
let kind = get_type_def_kind(db, type_decl_id);       // O(1)
let alias_origin = type_def_alias_origin(db, type_decl_id); // O(1)
```

#### 模式 3：声明推断

```rust
// 按位置查找声明
if let Some(decl) = find_compilation_decl_by_position(db, file_id, position) {
    // decl.summary.kind — SalsaDeclKindSummary
    // decl.decl_type — 推断的类型信息
}

// 按语法 ID 查找
if let Some(decl) = find_compilation_decl_by_syntax_id(db, file_id, syntax_id) {
    // ...
}
```

#### 模式 4：模块查询

```rust
// 获取模块信息
if let Some(module) = project_module_info(db, file_id) {
    println!("module: {}", module.full_module_name);
    println!("  export_target: {:?}", module.export_target);
    println!("  semantic_target: {:?}", module.semantic_target);
}
```

#### 模式 5：语义目标查询

```rust
let summary_db = db.get_summary_db();
if let Some(target) = summary_db.semantic().target().decl(file_id, decl_id) {
    // SalsaSemanticTargetInfoSummary — 语义解析后的目标信息
}
```

### 9. 新增 tracked 函数指南

当你需要在 Salsa 层新增查询时：

```rust
// 1. 在 tracked/mod.rs 中声明 #[salsa::tracked] 函数
#[salsa::tracked]
pub(crate) fn file_my_new_query(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    extra_param: SomeType,
) -> Option<Arc<SalsaMyNewSummary>> {
    let text = file.text(db);
    let chunk = parse_chunk(file.file_id(db), &text, &config.config(db));
    // ... 分析和查询逻辑 ...
    Some(Arc::new(result))
}

// 2. 在 facade.rs 对应的 facade 中添加桥接方法
impl SalsaSummaryFileQueries<'_> {
    pub fn my_new_query(&self, file_id: FileId, extra_param: SomeType) -> Option<Arc<SalsaMyNewSummary>> {
        tracked::file_my_new_query(self.db, file_id, extra_param)
    }
}

// 3. （可选）在 compilation 层添加 projection 函数
pub(crate) fn my_projection(db: &DbIndex, file_id: FileId) -> Option<Arc<SalsaMyNewSummary>> {
    db.get_summary_db().file().my_new_query(file_id, extra_param)
}
```

**关键规则**：
- Tracked 函数签名范式：`(db: &dyn SummaryDb, file: SummarySourceFileInput, config: SummaryConfigInput, ...其他参数)` — 前三个参数固定
- 返回类型优先 Arc 包装，以利用 Salsa 的 memoization 缓存
- 不要使用 `#[salsa::volatile]`（当前架构中不使用）
- 跨文件查询需要通过 `SummaryFileListInput` 追踪文件列表

### 10. 当前已知的限制和待改进项

| # | 问题 | 影响 | 计划 |
|---|---|---|---|
| A1 | Projection 层不在 Salsa 追踪内 | 跨文件查询不自动失效 | Phase 3 需要引入 `#[salsa::tracked]` 跨文件查询 |
| A2 | `RwLock<SalsaSummaryDatabase>` | 限制并行读、依赖追踪不完整 | 长期考虑去除 RwLock |
| A3 | VFS 重复（DbIndex + SalsaSummaryHost） | 测试路径可能不一致 | 统一 VFS 来源 |
| A4 | 解析器重复调用（parse_chunk 被多个 tracked 函数独立调用） | 缓存条目冗余 | 创建共享 `tracked_file_parsed_chunk` |
| A5 | 无 operator_index Salsa 替代 | infer_call 11 处无法迁移 | Phase 3a.1 |
| A6 | 无 type_cache Salsa 替代 | ~40 处 type_index.get_type_cache() | Phase 3a.2 |
| B1 | `pub use compilation::*` 泄漏 Salsa 类型 | 公共 API 过宽 | 逐步收紧 |
| B2 | `LuaCompilation::get_db()` → pub | 绕过 facade 直接访问 legacy index | Phase 3c |

### 11. 快速参考卡片

```
想做什么？                            → 调用哪个 API
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
查询某文件的类型定义                   → summary_db.doc().type_def_by_name(file_id, name)
查询某文件的全部泛型参数               → find_compilation_type_generic_params(db, type_decl_id)
查询 alias 的原始类型                  → type_def_alias_origin(db, type_decl_id)
判断类型是否 class/alias/enum          → type_def_is_class/is_alias/is_enum(db, type_decl_id)
查询模块信息                          → project_module_info(db, file_id)
按 require 路径查模块                  → find_module_by_require_path(db, "socket.core")
查询声明推断类型                       → find_compilation_decl_by_position(db, file_id, pos)
查询语义图 SCC                         → summary_db.semantic().file().graph_scc_index(file_id)
查询全局变量的类型                     → global_type(db, "print")
查询文件的成员索引                     → summary_db.file().members(file_id)
查询 doc 标签                         → summary_db.doc().tags(file_id)
查询签名解释                          → summary_db.doc().signature().explain(file_id, offset)
查询控制流                            → summary_db.flow().summary(file_id)
查询词法引用                          → summary_db.lexical().name_resolution(file_id, offset)
跨文件类型反向查找（O(1)）             → (通过 TypeDefReverseIndex — member.rs projection 函数)
```

# EmmyLuaAnalyzer Salsa 系统架构分析

> 生成日期: 2026-06-30 | 基于当前代码库分析

## 一、整体架构概览

项目处于从旧有的 `DbIndex` 架构向 **Salsa 增量计算架构** 的迁移过程中。当前存在两套并行的分析路径：

```
┌─────────────────────────────────────────────────────┐
│                   EmmyLua LSP                       │
│                  (语言服务器层)                        │
└───────────────────────┬─────────────────────────────┘
                        │
    ┌───────────────────┼───────────────────────────┐
    │                   │                           │
    ▼                   ▼                           ▼
┌────────────┐  ┌──────────────────┐   ┌──────────────────────┐
│ 旧版路径    │  │  Salsa DB (新)    │   │  SemanticModel (新)   │
│ DbIndex    │  │  增量计算引擎      │   │  单文件语义查询入口     │
│ (全局索引)  │  │                  │   │                      │
│            │  │  #[salsa::tracked]│   │  TypeQuery            │
│ 15个子索引  │  │  70+ 查询函数     │   │  DeclQuery            │
│ + VFS      │  │                  │   │  InferQuery           │
│ + Emmyrc   │  │  summary/analysis │   │  SigQuery             │
│            │  │  summary/query    │   │  MemberQuery          │
│ 持有 Arc<  │◄─┤  summary/summary  │   │  type_check/          │
│ SalsaDB>   │  │                  │   │  generic/             │
└────────────┘  └──────────────────┘   └──────────────────────┘
     ▲                    ▲                       ▲
     │                    │                       │
     └── 旧版 semantic/ 模块基于 DbIndex 进行语义查询──┘
     │
     └── 新版 semantic_model/ 模块直接读取 Salsa DB ──┘
```

### 核心库结构

| 库（Crate） | 用途 |
|-------------|------|
| `emmylua_parser` | Lua 语法解析器 (rowan AST) |
| `emmylua_parser_desc` | 文档注释解析器 (EmmyLua 注解) |
| **`emmylua_code_analysis`** | **核心分析库，包含 Salsa DB + 两套语义模型** |
| `emmylua_ls` | LSP 语言服务器 |
| `emmylua_check` | CLI 诊断检查工具 |
| `emmylua_doc_cli` | CLI 文档生成器 |
| `emmylua_formatter` | 代码格式化器 |

---

## 二、Salsa 增量计算系统的四层架构

Salsa 系统位于 `compilation/summary_builder/` 下，采用明确的分层设计：

```
┌─────────────────────────────────────────────────────┐
│  Layer 1: salsa_db (顶层 — 增量计算编排)              │
│  ┌───────────────────────────────────────────────┐  │
│  │ SalsaSummaryDatabase + #[salsa::db] trait     │  │
│  │   ├── inputs/     #[salsa::input] 输入        │  │
│  │   ├── tracked/    #[salsa::tracked] 查询函数   │  │
│  │   ├── facade.rs   公开查询门面 (7个门面)       │  │
│  │   └── tests/      集成测试                    │  │
│  └───────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────┤
│  Layer 2: analysis (中间层 — 纯事实提取)              │
│  ┌───────────────────────────────────────────────┐  │
│  │ AST → Summary 结构体的纯函数转换:              │  │
│  │   analyze_decl_summary()                      │  │
│  │   analyze_flow_summary()                      │  │
│  │   analyze_doc_summary()                       │  │
│  │   analyze_signature_summary()                 │  │
│  │   analyze_property_summary()                  │  │
│  │   analyze_use_site_summary()                  │  │
│  │   analyze_table_shape_summary()               │  │
│  └───────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────┤
│  Layer 3: query (中间层 — 索引构建与查询)             │
│  ┌───────────────────────────────────────────────┐  │
│  │ Summary → Index 结构的转换函数:                │  │
│  │   build_*_index()   构建二级索引               │  │
│  │   find_*_at/**in()  O(1) 查找                 │  │
│  │   collect_*()       聚合查询                  │  │
│  │   build_*_graph()   图结构构建                 │  │
│  └───────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────┤
│  Layer 4: summary (底层 — 不可变事实数据类型)         │
│  ┌───────────────────────────────────────────────┐  │
│  │ 14 个模块、100+ #[derive(salsa::Update)] 结构体 │  │
│  │   decl.rs      doc.rs      doc_type.rs        │  │
│  │   file.rs      flow.rs     member.rs          │  │
│  │   module.rs    property.rs signature.rs       │  │
│  │   table_shape.rs  use_site.rs                 │  │
│  │   semantic_graph.rs  semantic_solver.rs       │  │
│  │   type_def.rs  owner_binding.rs               │  │
│  └───────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────┘
```

### 依赖链

```
SummarySourceFileInput + SummaryConfigInput  (salsa input)
  │
  ├── tracked_file_decl_analysis (根基查询)
  │     ├── decl_tree_summary
  │     ├── global_summary
  │     └── member_summary
  │
  ├── tracked_file_doc_summary
  ├── tracked_file_flow_summary
  ├── tracked_file_signature_summary
  ├── tracked_file_property_summary
  ├── tracked_file_use_site_summary
  ├── tracked_file_table_shape_summary
  │
  ├── tracked_file_decl_type_query_index   (依赖上述多项)
  ├── tracked_file_member_type_query_index
  ├── tracked_file_flow_query_summary
  │
  ├── tracked_file_semantic_summary        (高层次聚合)
  ├── tracked_file_semantic_graph          (依赖几乎所有查询)
  ├── tracked_file_semantic_graph_scc_index
  │
  └── tracked_file_semantic_solver_*       (SCC + worklist 求解器)
```

---

## 三、关键组件详解

### 3.1 Salsa 输入 (`inputs/`)

```rust
// 每个文件一个输入实例，text 变化触发相关查询重新计算
#[salsa::input]
pub(crate) struct SummarySourceFileInput {
    pub(crate) file_id: FileId,
    pub(crate) path: Option<PathBuf>,
    pub(crate) text: String,          // salsa 跟踪其变化
    pub(crate) is_remote: bool,
}

// 全局配置输入
#[salsa::input]
pub(crate) struct SummaryConfigInput {
    pub(crate) config: SalsaSummaryConfig,  // 语言级别、特殊函数等
}
```

### 3.2 查询门面 (`facade.rs`)

7 个公开门面，每个提供特定领域的 API：

| 门面 | 访问方式 | 职责 |
|------|----------|------|
| `SalsaSummaryFileQueries` | `db.file()` | 声明树、全局、成员、属性、表格形状 |
| `SalsaSummaryDocQueries` | `db.doc()` | 文档摘要、标签、类型、owner 绑定 |
| `SalsaSummaryFlowQueries` | `db.flow()` | 控制流图：块、分支、循环、条件、边、可达性 |
| `SalsaSummaryLexicalQueries` | `db.lexical()` | 使用位点、名称/成员解析、引用 |
| `SalsaSummaryModuleQueries` | `db.module()` | 模块解析、导出 |
| `SalsaSummarySemanticQueries` | `db.semantic()` | 语义摘要、求解器 |
| `SalsaSummaryTypeQueries` | `db.types()` | 类型信息查询 |

### 3.3 控制流系统 (Flow Analysis)

控制流是类型收窄（type narrowing）的基础。整个 flow 系统的数据流：

```
LuaChunk (AST)
  │
  ▼ analyze_flow_summary()
SalsaFlowSummary (事实集合)
  │
  ├──► build_flow_exact_lookup_index() → SalsaFlowExactLookupIndex
  │      (按 offset 的二分查找索引)
  │
  └──► build_flow_query_summary() → SalsaFlowQuerySummary
         (完整的控制流图: blocks, statements, branches,
          loops, returns, breaks, **continues**, gotos, labels,
          + 边/链接/终端边)
         │
         ├──► build_condition_graph_summary()
         ├──► build_branch_graph_summary()
         ├──► build_loop_graph_summary()
         └──► build_terminal_graph_summary()
```

**`continue` 语句的控制流设计 (本次实现):**

```
continue 的 flow 图结构:
  Statement ──(StatementToTerminal)──► Continue(offset)
  Continue(offset) ──(TerminalToTarget)──► Block(loop_body)
  Continue(offset) ──(TerminalToUnreachable)──► Unreachable
  Block(loop_body) ──(LoopContinue)──► Condition/Loop  (已存在的回边)
```

与 `break` 对比:
- `break`: 终端目标 = 循环出口块 (parent of loop body)
- `continue`: 终端目标 = 循环体块 (re-enter)，随后通过 `LoopContinue` 边回到条件判断

### 3.4 语义求解器 (Semantic Solver)

最复杂的子系统，处理递归类型依赖（如相互递归函数）：

```
SalsaSemanticGraphSummary (依赖图)
  │
  ▼ build_semantic_graph_scc_index()
SCC Components (强连通分量)
  │
  ▼ build_semantic_solver_worklist()
Worklist (拓扑排序的任务队列)
  │
  ▼ step_semantic_solver_execution_with_signature_returns()
Iterative fixed-point computation
  │
  ▼ SalsaSemanticSolverComponentResultSummary
Resolved type information per component
```

---

## 四、新旧系统对比

| 维度 | 旧 DbIndex | 新 Salsa DB |
|------|-----------|-------------|
| 数据模型 | 可变全局索引 | 不可变、按文件增量缓存 |
| 查询粒度 | 全局扫描/查找 | 文件级、查询级细粒度缓存 |
| 失效策略 | 手动 `remove()`+重新分析 | 自动依赖追踪+按需重算 |
| 类型系统 | `LuaType` 内部类型 | `SalsaDocType*` 文档类型摘要 |
| 语义查询 | `semantic/` 目录 (基于 DbIndex) | `semantic_model/` 目录 (基于 Salsa DB) |
| 控制流 | `db_index/flow/` (FlowTree + FlowBinder) | `summary_builder/` (SalsaFlowSummary + 图查询) |

### 新旧语义模型对比

| | `semantic/` (旧) | `semantic_model/` (新) |
|---|---|---|
| 依赖 | `DbIndex` | `SalsaSummaryDatabase` 直接访问 |
| 推断缓存 | `LuaInferCache` (RefCell) | `InferCache` (RefCell + file_id) |
| 类型收窄 | 完整实现 (~2000行) | **大部分未迁移** |
| 子类型检查 | `type_check/` | `type_check/` (基于新类型系统) |
| 泛型求解 | `generic/` | `generic/` + `generic_checker.rs` |

---

## 五、已知缺失与待办事项

### 5.1 已完成 (本次实现)
- [x] `continue` 语句的控制流分析完整实现
  - `SalsaFlowContinueSummary` 数据结构
  - `SalsaFlowContinueLinkSummary` 链接结构
  - `SalsaFlowExactLookupIndex` 中的 `continues` + `by_continue_offset`
  - `build_flow_query_summary` 中的 `continue_links` + `terminal_edges`
  - `tracked_file_flow_continue` / `file_flow_continue` 查询函数
  - `continue_graph()` 终端图查询
  - 门面 API: `continue_at()`, `continue_graph()`

### 5.2 高优先级缺失

1. **类型收窄 (Flow-sensitive type narrowing) 未从旧系统迁移**
   - 旧系统: `semantic/infer/narrow/` 含 ~2000 行代码
   - 包括: `condition_flow/` (条件流), `narrow_type/` (类型收窄), `get_type_at_flow.rs` (按流位置查类型)
   - 新系统: `semantic_model/` 中无对应实现

2. **`infer_call_expr` 仅 13 行占位**
   - 函数调用类型推断未完成

3. **`lowered_node_to_lua_type` 不完整**
   - 文档类型到内部 LuaType 的转换缺少多种情况

4. **递归守卫缺失**
   - `salsa_inferring: RefCell<HashSet<(FileId, TextSize)>>` 在 `DbIndex` 中已存在
   - 新 `InferCache` 中的递归检测逻辑需要完善

### 5.3 中优先级缺失

5. **双重解析路径**
   - 模块导出解析同时存在于旧 (`module_query.rs`) 和新 (`module/` 查询) 系统中

6. **未使用的类型**
   - `CompilationDeclIndex` 已定义但未构造
   - `GenericBindings` 已定义但未使用

7. **Salsa 测试覆盖不均衡**
   - Flow 测试: 5 个 (含刚添加的 continue 测试)
   - 整体 154 个 salsa_db 测试，集中在类型查询、属性、语义求解器

### 5.4 架构债务

8. **`DbIndex` 与 `SalsaSummaryDatabase` 的双向依赖**
   - `DbIndex` 持有 `Arc<SalsaSummaryDatabase>` 用于类型反向索引
   - `SemanticModel` 同时访问两者，可能导致缓存不一致

9. **Config 传递模式**
   - 每个 `#[salsa::tracked]` 函数都需要 `config: SummaryConfigInput` 参数
   - 可通过 salsa jar 或 ambient config 简化

10. **语法树缓存**
    - 语法树在 `SalsaSummaryDatabase.syntax_trees: HashMap` 中手动管理
    - 未使用 salsa 的输入机制，意味着树本身不参与增量追踪

---

## 六、迁移路线建议

### Phase 1 (当前): 补齐新系统的核心能力
- [ ] 迁移类型收窄逻辑到 `semantic_model/`
- [ ] 完善 `infer_call_expr`
- [ ] 完成 `lowered_node_to_lua_type`

### Phase 2: 统一入口
- [ ] 所有诊断检查器切换到 `SemanticModel`
- [ ] 移除诊断中对 `DbIndex` 的直接依赖

### Phase 3: 消除旧系统
- [ ] `DbIndex` 的数据迁移到 Salsa DB
- [ ] 删除 `semantic/` 目录
- [ ] 删除 `db_index/` 中已迁移的索引

### Phase 4: 优化
- [ ] 解决语法树缓存的增量问题
- [ ] 减少 config 参数传递的模板代码
- [ ] 提升 SCC 求解器的大文件性能

---

## 七、关键文件索引

| 文件 | 行数 | 说明 |
|------|------|------|
| `salsa_db/tracked/mod.rs` | ~2682 | 核心 `#[salsa::tracked]` 查询函数 |
| `salsa_db/facade.rs` | ~1404 | 查询门面 API |
| `query/flow.rs` | ~2097 | 控制流图的索引构建与查询 |
| `analysis/flow.rs` | ~406 | AST → FlowSummary 的转换 |
| `summary/flow.rs` | ~365 | 控制流数据类型定义 |
| `semantic_model/mod.rs` | ~200 | SemanticModel 入口点 |
| `semantic_model/infer/mod.rs` | ~800 | 新版类型推断 |
| `semantic/infer/narrow/` | ~2000 | 旧版类型收窄 (待迁移) |
| `db_index/mod.rs` | ~600 | 旧版全局索引容器 |

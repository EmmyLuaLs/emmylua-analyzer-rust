# SemanticModel 重设计文档

## 一、现状分析

### 1.1 当前架构问题

```
EmmyLuaAnalysis
 └─ LuaCompilation
     └─ DbIndex  (巨型容器 — 15+ 子索引 + VFS + SalsaSummaryDatabase)
         ├─ LuaDeclIndex
         ├─ LuaReferenceIndex
         ├─ LuaTypeIndex
         ├─ LuaModuleIndex
         ├─ LuaMemberIndex
         ├─ LuaPropertyIndex
         ├─ LuaSignatureIndex
         ├─ DiagnosticIndex
         ├─ LuaOperatorIndex
         ├─ LuaFlowIndex
         ├─ LuaMetatableIndex
         ├─ LuaGlobalIndex
         ├─ JsonSchemaIndex
         ├─ LuaDependencyIndex
         ├─ Vfs
         └─ SalsaSummaryDatabase (RwLock<SalsaSummaryDatabase>)
              ├─ File queries (decl_tree, globals, members, properties, table_shapes...)
              ├─ Doc queries (tags, types, signatures, owner_resolves...)
              ├─ Lexical queries (name_resolution, references, call_use...)
              ├─ Flow queries (cfg, branches, conditions, loops...)
              ├─ Module queries (exports, resolve_index...)
              ├─ Semantic queries (graph, solver, SCC, decl/member summaries...)
              └─ Type queries (decl_type, member_type, name_type...)
```

**核心问题：**

1. **DbIndex 是上帝对象** — 15+ 个子索引塞在一个 struct 里，所有自由函数都直接拿 `&DbIndex`
2. **SemanticModel 是薄壳** — 现有 350 行基本都是 `self.db` 透传给自由函数，自身没有逻辑
3. **Salsa 被锁在 RwLock 里** — `SalsaSummaryDatabase` 作为 `DbIndex` 的字段存在，通过 `RwLock` 访问，完全丧失了 salsa 的增量优势
4. **两套系统混用** — `salsa_inferring` 递归守卫在 salsa 和 legacy cache 之间切换，脆弱且难维护
5. **pub use 层级混乱** — lib.rs 宣称 DbIndex 是 Tier 3 (Legacy)，但 `SemanticModel::get_db()` 仍然暴露它

### 1.2 旧 SemanticModel 对外 API 全景

```
SemanticModel<'a> {
    // --- 文件/文档 ---
    get_document()           -> LuaDocument
    get_file_id()            -> FileId
    get_root()               -> &LuaChunk
    get_emmyrc()             -> &Emmyrc
    get_file_parse_error()   -> Option<Vec<LuaParseError>>

    // --- 模块 ---
    get_module()                        -> Option<CompilationModuleInfo>
    get_module_by_file_id(FileId)       -> Option<CompilationModuleInfo>
    find_module_by_require_path(&str)   -> Option<CompilationModuleInfo>
    resolve_module_export_type(FileId)  -> Option<LuaType>

    // --- 类型推断 ---
    infer_expr(LuaExpr)                 -> Result<LuaType, InferFailReason>
    infer_table_should_be(LuaTableExpr) -> Option<LuaType>
    infer_call_expr_func(...)           -> Option<Arc<LuaFunctionType>>
    infer_expr_list_types(...)          -> Vec<(LuaType, TextRange)>
    infer_bind_value_type(LuaExpr)      -> Option<LuaType>
    infer_member_type(...)              -> Result<LuaType, InferFailReason>
    get_index_decl_type(LuaIndexExpr)   -> Option<LuaType>

    // --- 成员查询 ---
    get_member_infos(&LuaType)                            -> Option<Vec<LuaMemberInfo>>
    get_member_info_with_key(...)                         -> Option<Vec<LuaMemberInfo>>
    get_member_info_map(&LuaType)                         -> Option<HashMap<...>>
    get_member_key(&LuaIndexKey)                          -> Option<LuaMemberKey>
    get_member_origin_owner(LuaMemberId)                  -> Option<LuaSemanticDeclId>

    // --- 语义信息 (hover / goto-def 核心) ---
    get_semantic_info(NodeOrToken)          -> Option<SemanticInfo>
    find_decl(NodeOrToken, SemanticDeclLevel) -> Option<LuaSemanticDeclId>

    // --- 类型检查 ---
    type_check(&LuaType, &LuaType)         -> TypeCheckResult
    type_check_detail(&LuaType, &LuaType)  -> TypeCheckResult
    is_sub_type_of(...)                    -> bool

    // --- 引用/可见性 ---
    is_reference_to(node, decl_id, level)  -> bool
    is_semantic_visible(token, decl_id)    -> bool

    // --- 泛型 ---
    get_type_generic_params(&LuaTypeDeclId) -> Option<Vec<CompilationGenericParamInfo>>

    // --- 类型查询 ---
    get_type(LuaTypeOwner)                 -> LuaType

    // --- 危险：暴露内部 ---
    get_db()        -> &DbIndex       // ❌ 破坏封装
    get_cache()     -> &RefCell<...>  // ❌ 破坏封装
}
```

### 1.3 消费者分析

主要消费者是 `diagnostic/checker/` 下的 ~30 个 checker，它们使用的 SemanticModel 方法集中在：

| 使用频率 | 方法 |
|---------|------|
| 极高 | `get_root()`, `get_file_id()`, `get_emmyrc()` |
| 高 | `infer_expr()`, `find_decl()`, `get_semantic_info()`, `type_check()` |
| 中 | `get_member_infos()`, `get_member_info_with_key()`, `infer_call_expr_func()` |
| 低 | 其余方法 |

---

## 二、设计目标

1. **Salsa-native** — SemanticModel 直接持有 salsa db 的引用，利用增量计算
2. **零 DbIndex 泄漏** — 外部永远不需要知道 DbIndex 的存在
3. **渐进迁移** — `semantic_model/` 镜像旧 `semantic/` 的目录结构，完成一个子模块切换一个
4. **简洁 API** — 方法数量减少 30%+，合并相关功能
5. **可测试** — 每个子模块可独立单元测试

---

## 三、新架构设计

### 3.1 核心结构

```rust
/// 单文件的语义视图。
/// 
/// 生命周期 'db 绑定到 salsa 数据库，确保查询可以被 memoize。
pub struct SemanticModel<'db> {
    file_id: FileId,
    db: &'db SalsaSummaryDatabase,   // 直接引用 salsa db，不再经过 DbIndex
    emmyrc: Arc<Emmyrc>,
    root: LuaChunk,
    // 缓存放在 salsa 的 tracked functions 中，不再需要 RefCell<LuaInferCache>
}
```

### 3.2 与旧结构的关键区别

| 维度 | 旧 SemanticModel | 新 SemanticModel |
|------|-----------------|-----------------|
| 数据库 | `&DbIndex` (15+ 子索引) | `&SalsaSummaryDatabase` (salsa 原生) |
| 缓存 | `RefCell<LuaInferCache>` — 手动管理 | Salsa `#[salsa::tracked]` — 自动增量 |
| 类型推断 | 自由函数 `infer_expr(db, cache, expr)` | 方法 `self.infer_expr(expr)` -> salsa tracked |
| 成员查询 | `find_members_in_scope(db, file_id, ...)` | `self.members().find(prefix_type)` |
| 封装 | `get_db()` 暴露内部 | 完全封闭，不暴露 db |

### 3.3 模块划分

```
semantic_model/
├── mod.rs              # SemanticModel 主结构 + 公开 API
├── DESIGN.md           # 本文档
├── infer/              # 类型推断
│   ├── mod.rs          #   infer_expr, infer_type, ...
│   ├── expr.rs         #   表达式推断
│   ├── call.rs         #   函数调用推断
│   ├── table.rs        #   表推断
│   └── narrow.rs       #   类型窄化 (type narrowing)
├── member/             # 成员查询
│   ├── mod.rs          #   find_members, get_member_info
│   └── origin.rs       #   成员来源追溯
├── type_check/         # 类型检查
│   ├── mod.rs          #   is_sub_type_of, check_type_compact
│   └── complex/        #   复杂类型检查 (array, object, ...)
├── reference/          # 引用分析
│   └── mod.rs          #   find_references, is_reference_to
├── visibility/         # 可见性检查
│   └── mod.rs          #   check_visibility
├── generic/            # 泛型处理
│   └── mod.rs          #   instantiate_generic, get_generic_params
├── overload/           # 重载解析
│   └── mod.rs          #   resolve_overload
└── semantic_info/      # 语义信息 (hover / goto-def / 补全)
    └── mod.rs          #   get_semantic_info, find_decl
```

### 3.4 子模块封装模式

每个子模块使用独立的 query struct 封装：

```rust
// semantic_model/member/mod.rs

/// 成员查询的入口，按需创建
pub struct MemberQuery<'db> {
    db: &'db SalsaSummaryDatabase,
    file_id: FileId,
}

impl<'db> MemberQuery<'db> {
    pub fn find(&self, prefix_type: &LuaType) -> Option<Vec<LuaMemberInfo>> {
        // 直接调用 salsa tracked functions
        // salsa 自动处理缓存和增量失效
        ...
    }

    pub fn find_with_key(
        &self,
        prefix_type: &LuaType,
        key: LuaMemberKey,
    ) -> Option<Vec<LuaMemberInfo>> {
        ...
    }
}
```

SemanticModel 上提供便捷方法：

```rust
impl<'db> SemanticModel<'db> {
    /// 获取成员查询器
    pub fn members(&self) -> MemberQuery<'db> {
        MemberQuery { db: self.db, file_id: self.file_id }
    }

    /// 快捷方法：直接查成员
    pub fn find_members(&self, prefix_type: &LuaType) -> Option<Vec<LuaMemberInfo>> {
        self.members().find(prefix_type)
    }
}
```

### 3.5 类型推断架构

这是最复杂的部分。当前 `infer/` 有大量自由函数。新设计：

```rust
// semantic_model/infer/mod.rs

pub struct InferQuery<'db> {
    db: &'db SalsaSummaryDatabase,
    file_id: FileId,
    cache: InferCache,  // 轻量的、非跨文件的本地缓存
}

impl<'db> InferQuery<'db> {
    /// 推断表达式类型 —— 最核心的方法
    pub fn infer_expr(&self, expr: LuaExpr) -> Result<LuaType, InferFailReason> {
        // 优先查 salsa 中已有的类型信息
        // 兜底使用 AST 遍历推断
        ...
    }

    /// 推断调用表达式的目标函数
    pub fn infer_call_target(&self, call: LuaCallExpr, arg_count: Option<usize>) 
        -> Option<Arc<LuaFunctionType>> {
        ...
    }
}
```

**关键设计决策：双层查询**

```
infer_expr(expr)
  │
  ├─ 1. 查 Salsa 类型索引 (快速路径)
  │     ├─ SalsaSummaryTypeQueries::name(file_id, offset)
  │     ├─ SalsaSummaryTypeQueries::member(file_id, target)
  │     └─ SalsaSummaryTypeQueries::decl(file_id, decl_id)
  │     命中 → 直接返回 ✅
  │
  └─ 2. AST 遍历推断 (慢速路径，仅 salsa 无缓存时)
        └─ 递归遍历子表达式，推断类型
          结果写入本地 InferCache
```

---

## 四、迁移计划

### Phase 1：搭建骨架 ✅
- [x] 实现 `SemanticModel` 基础结构（持有 `Arc<RwLock<SalsaSummaryDatabase>>`）
- [x] `get_root()`, `get_file_id()`, `get_emmyrc()` — 纯数据方法
- [x] `get_document()`, `get_file_parse_error()` — VFS 桥接
- [x] 编译通过，零旧代码影响

### Phase 2：成员查询 + 推断骨架 ✅
- [x] `members()` → `MemberQuery` 封装（`all()`, `list()`, `by_syntax_id()`, `type_of()`）
- [x] `infer()` → `InferQuery` 封装（salsa-first 双路径）
- [x] `infer_expr()` dispatcher + 本地 `InferCache`
- [x] `infer_literal()` 完整实现
- [x] `infer_closure()` 完整实现

### Phase 3：名称推断 ✅
- [x] `infer_name()` — 完整链路：salsa types::name() → named_type_names → LuaType
- [x] `resolve_decl_type()` — SalsaDeclTypeInfoSummary → LuaType 转换
- [x] `resolve_single_named_type()` — 基础类型 + 自定义类型 + 泛型处理
- [x] `infer_global_name()` — 全局函数/变量查找
- [x] 可见性处理（Private→local，其余→global）

### Phase 4：表达式推断 ✅
- [x] `infer_call()` — 函数调用推断（DocFunction/Signature/Union/Generic/Intersection）
- [x] `infer_index()` — 索引表达式推断（salsa fast-path + 完整前缀类型分发）
- [x] `infer_table()` — 表推断（Array 类型）
- [x] `infer_member_type()` — 成员类型推断（13 种前缀类型全覆盖）← 新增
- [ ] `infer_binary()` / `infer_unary()` — 后续
- [ ] `infer_expr_list_types()` / `infer_bind_value_type()` — 后续

### Phase 5：类型检查 ✅ — 647 行
- [x] `check_type_compact()` — 15 种 source 类型 × compact 类型的兼容矩阵
- [x] `check_type_compact_detail()` — 详细模式
- [x] `check_ref_source()` — Ref/Def source → is_sub_type_of + 基类映射
- [x] `check_union_source()` — 所有成员必须匹配
- [x] `check_intersection_source()` — 任一成员匹配即可
- [x] `check_array_source()` — 元素类型对比
- [x] `check_object_source()` — 字段级对比
- [x] `check_tuple_source()` — 位置级对比
- [x] `check_generic_source()` — 泛型展开后对比
- [x] `check_table_generic_source()` — TableGeneric 对比
- [x] `check_func_source()` — DocFunction/Signature 对比
- [x] `is_sub_type_of()` — 相等 + 同名检查（完整类层次遍历后续 phase）

### Phase 6：其余 + 切换
- [ ] `visibility/`, `reference/`, `generic/`
- [ ] 修改 `LuaCompilation::get_semantic_model()` 返回新类型
- [ ] 逐个 checker 验证后删除旧 `semantic/`

---

## 编码规范（新增）

这些规范是在开发过程中迭代出来的：

1. **禁止 `unwrap()`** — 项目有 `#![deny(clippy::unwrap_used)]`。用 `expect("reason")` 替代。
2. **禁止 `crate::` 前缀** — 文件顶部用 `use crate::...` 导入，函数签名中不出现 `crate::`。
3. **禁止 `super::`** — 同文件内不需要，跨子模块也用 top-level `use` 导入。
4. **结构体不加无意义的生命周期** — 如果字段是 `Arc<T>`（自有），不需要 `'db`。只有在持有引用时才需要生命周期。
5. **`get_position()` 替代 `syntax().text_range().start()`** — `LuaAstNode::get_position()` 更简洁。
6. **SmolStr 参数优先 `&str`** — facade 方法接受 `&str` 后内部转换，避免调用方不必要的拷贝。
7. **RwLock 读取封装** — 用 `read_db()` 辅助方法返回 `impl Deref<Target = SalsaSummaryDatabase>`，简化子模块中的锁访问。

---

## 五、API 设计原则

1. **方法短** — 每个方法不超过 20 行，复杂逻辑拆到子模块
2. **参数具体** — 不用 `NodeOrToken` 这种联合类型，拆成两个方法
3. **返回清晰** — `Option<T>` 表示可能不存在，`Result<T, E>` 表示可能失败
4. **不暴露内部** — 没有 `get_db()`, `get_cache()` 等方法
5. **命名一致** — 查询方法统一用 `find_` (可能不存在) / `get_` (一定存在) / `infer_` (需要计算)
6. **LuaType 零拷贝** — 返回 `LuaType` (Clonable) 而非 `&LuaType`，简化生命周期

---

## 六、关键类型映射

| 旧类型 (db_index) | 新来源 (salsa) |
|------------------|---------------|
| `LuaDeclIndex` | `SalsaSummaryFileQueries::decl_tree()` |
| `LuaMemberIndex` | `SalsaSummaryFileQueries::members()` |
| `LuaTypeIndex` | `SalsaSummaryTypeQueries` |
| `LuaReferenceIndex` | `SalsaSummaryLexicalQueries::decl_references()` |
| `LuaSignatureIndex` | `SalsaSummaryDocSignatureQueries` |
| `LuaPropertyIndex` | `SalsaSummaryFileQueries::properties*()` |
| `LuaFlowIndex` | `SalsaSummaryFlowQueries` |
| `LuaModuleIndex` | `SalsaSummaryModuleQueries` |

---

## 七、不做什么

1. **不引入新的 trait 抽象层** — Rust trait 在大型项目中会增加编译时间和复杂度，直接用 struct + 方法
2. **不急于优化性能** — 先用最简单的方式实现正确性，salsa 自带 memoization 解决大部分性能问题
3. **不同时维护两套** — 新模块完成一个，验证通过后立即删除旧代码对应部分，避免同步负担
4. **不把所有东西都变成 salsa tracked** — 仅对跨文件、需要增量更新的查询使用 salsa；纯 AST 遍历的局部计算用普通方法

---

## 八、旧代码利用策略

旧 `semantic/` 目录下的代码**仅作为参考**，不直接复用：

- **算法逻辑可以借鉴** — 类型窄化、重载解析等复杂算法，读懂后在新架构中重写
- **数据结构尽量复用** — `LuaType`, `LuaMemberKey`, `LuaMemberInfo` 等类型定义保持不变
- **自由函数不迁移** — 旧代码中所有接受 `&DbIndex` 的自由函数都废弃，改为 SemanticModel 的方法或子模块方法
- **测试用例迁移** — 将旧的测试逻辑适配到新 API 后保留

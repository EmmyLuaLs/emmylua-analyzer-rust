# SemanticModel 重设计文档 v3

> 更新日期：2026-06-26
> 测试状态：270/375 pass（72%），105 个诊断测试待修复

## 整体架构

```
┌──────────────────────────────────────────────────────────────┐
│                    诊断层 (diagnostic/)                       │
│  check_file(context, model) → 38 个独立 checker 函数           │
│  全部使用新 SemanticModel，0 个依赖旧 DbIndex                   │
├──────────────────────────────────────────────────────────────┤
│                    语义模型 (semantic_model/)                  │
│  SemanticModel ── 单文件入口，持有 SalsaSummaryDatabase        │
│  ├── InferQuery    —— 类型推断（名称、调用、索引、表、二元等）    │
│  ├── MemberQuery   —— 成员查找                                 │
│  ├── TypeQuery     —— 类型定义、属性、跨文件查询                  │
│  ├── DeclQuery     —— 声明引用、可见性                           │
│  ├── SigQuery      —— 签名解释、泛型参数、lowered type 转换       │
│  ├── type_check/   —— 类型兼容性检查（15 种类型组合矩阵）         │
│  ├── reference/    —— 引用解析（名称→声明）                      │
│  └── visibility/   —— 可见性判断                                │
├──────────────────────────────────────────────────────────────┤
│                   Salsa 数据层 (summary_builder/)               │
│  ┌─────────────────┬────────────────┬──────────────────────┐ │
│  │  analysis/ (写)  │  query/ (计算)  │  tracked/ (缓存)     │ │
│  │  AST → Summary   │  索引构建+查找  │  #[salsa::tracked]   │ │
│  └─────────────────┴────────────────┴──────────────────────┘ │
│  SalsaSummaryDatabase — 增量计算 + 自动缓存失效                 │
└──────────────────────────────────────────────────────────────┘
```

## 核心完成度矩阵

| 模块 | 状态 | 通过率 | 关键缺口 |
|------|------|--------|----------|
| **summary 数据结构** | ✅ 完成 | — | decl, doc, flow, member, property, signature 等 15 个子模块 |
| **analysis 层** | ✅ 完成 | — | AST → Summary 的解析全部就绪 |
| **query 层** | ✅ 完成 | — | 16 个查询模块，索引构建+查找 |
| **tracked 层** | ✅ 完成 | — | ~88 个 `#[salsa::tracked]` 函数 |
| **facade 层** | ✅ 完成 | — | 10 个查询结构体，165+ 公共方法 |
| **类型检查 (type_check)** | ✅ 完成 | — | `check_type_compact` 覆盖 15 种类型组合 |
| **名称推断 (infer_name)** | ⚠️ 基本完成 | — | 缺流敏感窄化，双路径冗余待统一 |
| **字面量/闭包/一元/二元** | ✅ 完成 | — | 完整实现 |
| **索引表达式 (IndexExpr)** | ⚠️ 基本完成 | — | 委托 infer_member_type，缺流敏感分析 |
| **成员类型推断 (infer_member_type)** | ⚠️ 基本完成 | — | enum/class 成员查找可工作，缺 ModuleRef/TplRef 支持 |
| **表推断 (infer_table_expr)** | ❌ 部分完成 | — | 仅区分 array/table，未调用 infer_table_should_be |
| **调用推断 (infer_call_expr)** | ❌ 严重不足 | — | 13 行 stub，未调用 infer_call_expr_func |
| **重载解析** | ❌ 缺失 | — | 旧系统 ~700 行，新系统完全未移植 |
| **泛型实例化** | ❌ 缺失 | — | 旧系统 ~700 行，新系统完全未移植 |
| **流敏感窄化** | ❌ 缺失 | — | 旧系统 ~2000 行，新系统完全未移植 |
| **require() 特殊处理** | ❌ 缺失 | — | `local x = require("mod")` 不工作 |
| **setmetatable() 特殊处理** | ❌ 缺失 | — | 不合并 metatable |

## 推理管线（当前实际实现）

### resolve_decl_type（声明类型 → LuaType）

```
resolve_decl_type_depth(decl_type, depth=0)
├── Priority 0.4: named_type_names
│   └── resolve_named_types → resolve_single_named_type
│       ├── 内置类型 (nil/any/boolean/string/number/integer/function/table/thread/userdata)
│       └── TypeQuery::get_def_with_file → 跨文件查找 → LuaType::Ref/Def
├── Priority 1.5: explicit_type_offsets
│   ├── resolved_type_by_key → lowered_node_to_lua_type  (优先)
│   └── lowered_type_by_key → lowered_node_to_lua_type  (回退)
├── Priority 0: value_signature_offset → LuaType::Signature
├── Priority 0.5: value_expr 非简单表达式 → LuaType::Signature
├── Priority 0.6: value_expr 是 NameExpr → 追踪被引用变量的类型
│   ├── Path A: lexical().name_resolution_by_syntax_id → decl 类型
│   └── Path B: types().name() → decl_type 回退
└── 返回 None
```

### infer_name（NameExpr → LuaType）

```
infer_name(name_expr)
├── self / _G 特殊处理
├── closure 在 decl 范围内 → LuaType::Signature
├── db.types().name() → resolve_decl_type（复用上面的链路）
├── resolve_doc_type_for_decl → @type 注解
├── db.lexical().use_at() → 词法解析 → decl 类型
├── decl_tree 闭包查找 → LuaType::Signature
├── resolve_type_annotation_for_name → AST 扫描 @type 注释
├── resolve_doc_type_by_name → decl_tree + doc type_tags
└── infer_global_name → 全局类型/函数/变量
```

### infer_call_expr（当前严重不足）

```
infer_call_expr(call_expr)                     ← 仅有 13 行
├── 推断 prefix_expr 类型
└── extract_return_type
    ├── LuaType::Function → Any
    ├── LuaType::DocFunction → func.get_ret()
    ├── LuaType::Signature → return_type()
    └── LuaType::Union → 各分支提取后 union

缺失（旧系统均有）：
├── infer_call_expr_func → 完整签名匹配   ← 存在但未被调用！
├── 重载解析 (resolve_signature)          ← 完全缺失
├── 泛型实例化 (instantiate_func_generic)  ← 完全缺失
├── require() 特殊处理                    ← 完全缺失
├── setmetatable() 特殊处理               ← 完全缺失
└── self 类型绑定 (colon call)             ← 完全缺失
```

## 类型系统数据流

### Param 类型绑定（当前实现）

```
@param p Enum
  → doc parsing: SalsaDocParamSummary { type_offset: Enum_key }
  → collect_decl_explicit_type_offsets: 匹配 signature.owner + name → explicit_type_offsets
  → collect_decl_named_type_names: 查 lowered_type → 若为简单 Name → named_type_names
  → resolve_decl_type: Priority 0.4 命中 named_type_names → 跨文件 resolve_single_named_type
```

`named_type_names` 对 Param 和 Local/Global 的语义不同：
- **Local/Global**：声明本身就是该类型定义（`---@class Foo\nlocal foo = {}`）
- **Param**：声明被标注为该类型（`---@param p Foo`）

### 跨文件类型查找

- `TypeQuery::get_kind` / `get_def_with_file` → 遍历 `db.file_ids()`
- `resolve_single_named_type` → 先查当前文件，再遍历其他文件
- `WorkspaceMemberIndex` / `WorkspaceTypeIndex` → 跨文件聚合所有 properties/type_defs

## 已知架构问题

### 1. infer_call_expr 是 13 行 stub
`infer/call.rs:9-13` 从未调用 `infer_call_expr_func`（`mod.rs:1143`），导致所有函数调用只返回最粗糙的返回类型，没有参数匹配、重载选择、泛型实例化。

**修复方案**：在 `call.rs` 中调用 `infer_call_expr_func`，统一调用路径。

### 2. dual resolution paths
`resolve_decl_type_depth` 和 `infer_name` 有大量重复逻辑。两者都查找 `db.types().name()`、`named_type_names`、closure in decl 模式。

**修复方案**：`resolve_decl_type_depth` 作为唯一类型解析引擎，`infer_name` 只做名称→声明解析后委托。

### 3. lowered_node_to_lua_type 不完整
`infer/mod.rs:1080-1124` 只处理 `Unknown` / `Name` / `Array` / `Variadic` / `Literal`。缺少 `Function` / `Object` / `Generic` / `Nullable` / `Union` 等变体。

**修复方案**：扩展函数覆盖所有 `SalsaDocTypeLoweredKind` 变体。

### 4. 无递归推断守卫
`InferCache::mark_computing` 被标记 `#[allow(dead_code)]`，递归推断保护未启用。

### 5. 流敏感类型窄化完全缺失
旧系统 ~2000 行的 `narrow/` 模块未被移植。`if x ~= nil then print(#x) end` 无法窄化 `x` 的类型。

## 编码规范

1. 禁止 `unwrap()` — `#![deny(clippy::unwrap_used)]`
2. 被使用的变量不加 `_` 前缀 — `_db` → `db`
3. 禁止 `super::` — 跨子模块用 top-level `use`
4. `get_position()` 替代 `syntax().text_range().start()`
5. SmolStr 参数优先 `&str`

## 行动计划

| 优先级 | 任务 | 预估影响 |
|--------|------|----------|
| **P0** | 统一调用推断：`call.rs` 调用 `infer_call_expr_func` | 修复 ~40 个测试（param_type_check, missing_parameter, redundant_parameter, call_non_callable） |
| **P0** | 扩展 `lowered_node_to_lua_type` | 修复基于 doc type 的类型解析 |
| **P1** | 移植重载解析 | 修复 `@overload` 场景 |
| **P1** | 移植泛型实例化 | 修复泛型函数调用 |
| **P1** | 启用递归推断守卫 | 防止栈溢出 |
| **P2** | 表推断接入 `infer_table_should_be` | 修复表类型推断 |
| **P2** | 流敏感窄化 (Phase 1: nil 检查) | 修复条件窄化 |
| **P3** | 统一 dual resolution paths | 减少维护负担 |
| **P3** | require() / setmetatable() 特殊处理 | 修复模块/元表场景 |

# Workspace Index 与 `resolve_member` 重设计

> 状态：设计稿 / 待实现
> 目标读者：后续接手该模块的开发者

## 1. 背景与问题

当前 `resolve_member` 是语言服务器整体性能的主要瓶颈之一。

它不是单纯“某个 checker 慢”，而是几乎所有需要成员解析的功能都会经过它：

- 类型推断
- flow narrowing
- deprecated 检查
- undefined field / access invisible
- completion / hover
- parameter type check

目前 `resolve_member` 每次都要执行大量重复且昂贵的步骤：

1. 从 `IndexExpr` 提取 owner + key；
2. 尝试同文件成员；
3. 尝试同文件 class 定义；
4. 推断 prefix 类型；
5. 展开 owner identity set；
6. 反复扫描 `members_of_owner` / `members_of_owner_named`；
7. 查找 runtime self member；
8. require module owner；
9. 继承 / 类型投影 fallback。

其中很多步骤本质是“查表”就能解决的，但当前 workspace index 没有为这些查询提供直接可用的哈希索引，导致每次都在做数组扫描 + 多次 fallback。

### 现状中的具体问题

- `WorkspaceMemberIndex` 只有 `by_owner: HashMap<SemanticId, Arc<[MemberRef]>>`
  - 查询 `owner + key` 时仍要拿回整个数组再线性过滤；
- `WorkspaceDeclIndex.runtime_values` 是 `Vec`
  - `resolve_owner_set` 里通过 `iter().filter()` 扫描同名 runtime value；
- `resolve_member_impl` 是多阶段线性尝试，缺少一个“预解析好的 member use”缓存；
- `file_references` 虽然做了 per-file 的 member use 解析，但：
  - 只保存 member id；
  - 没有成为 `resolve_member` 的主路径；
  - 仍依赖旧的数组扫描型 workspace index；
- `SemanticModel` 的高频查询没有统一消费一个“已经跨文件解析好的结果”。

## 2. 目标

设计并实现一个面向查询的 workspace 缓存体系，使：

- 跨文件 member / type / decl 查询是 **O(1) 或接近 O(1)** 的索引查找；
- `resolve_member` 的绝大多数调用通过 **预计算好的 per-file 结果** 直接返回；
- 只有真正需要类型推断的少数表达式才走慢路径；
- 文件更新时：
  - 只重算该文件的 per-file index；
  - 只重算对应 shard；
  - workspace index 只 merge 少量 shard 的哈希片段；
- 后续 checker 不再各自重复“从语法到语义身份”的解析。

## 3. 非目标

- 不追求把所有语义查询塞进 salsa；
- 不把 `LuaType` / `CallSiteAnalysis` 等高频复杂类型作为 salsa key/value；
- 不重新引入前向 flow 的大规模预计算；
- 不把 `resolve_member` 的慢路径消灭到零——只让常规路径变快。

## 4. 总体架构

```text
┌──────────────────────────────────────────────────────────┐
│                     WorkspaceIndex                        │
│  per workspace 的跨文件直查索引                             │
│  - global decl / type / runtime value                     │
│  - member by owner / owner+key                            │
│  - type member by type / type+key                         │
│  - deprecated fast paths                                  │
└──────────────────────────────────────────────────────────┘
                          │ 依赖
                          ▼
┌──────────────────────────────────────────────────────────┐
│                   FileSemanticIndex                       │
│  per file 的语义预计算结果                                 │
│  - name use -> local/global/unresolved                    │
│  - member use -> ResolvedMember                           │
│  - 简单类型/成员关系缓存                                    │
└──────────────────────────────────────────────────────────┘
                          │ 消费
                          ▼
┌──────────────────────────────────────────────────────────┐
│                     SemanticModel                         │
│  短生命周期、只处理真正需要推断的少数情况                    │
└──────────────────────────────────────────────────────────┘
```

### 4.1 WorkspaceIndex

每个 workspace 一个。由多个 shard 的哈希片段合并而成。

```rust
struct WorkspaceIndex {
    // name -> 全局声明 / 类型 / runtime value
    global_decls: HashMap<SmolStr, SemanticId>,
    global_types: HashMap<SmolStr, SemanticId>,
    runtime_by_name: HashMap<SmolStr, Vec<SemanticId>>,

    // 身份成员索引
    members_by_owner: HashMap<SemanticId, MemberBucket>,
    members_by_owner_name: HashMap<(SemanticId, SmolStr), MemberBucket>,

    // 类型成员索引
    type_members_by_name: HashMap<(SemanticId, SmolStr), MemberBucket>,

    // deprecated 专用快路径
    deprecated_members_by_owner_name: HashMap<(SemanticId, SmolStr), MemberBucket>,
}

struct MemberBucket {
    members: Arc<[MemberRef]>,
}
```

核心原则：

- 所有跨文件查询直接命中哈希表；
- 不再在查询路径中 `iter().filter()` 扫描数组；
- 一个 owner / type 的所有成员以 `MemberBucket` 形式保存，可安全共享。

### 4.2 FileSemanticIndex

每个文件一个，salsa tracked query。

```rust
struct FileSemanticIndex {
    // 与 facts.name_uses 对齐
    name_use_resolution: Vec<NameResolution>,

    // key = LuaSyntaxId of IndexExpr
    member_use_resolution: HashMap<LuaSyntaxId, ResolvedMember>,

    // 可供 checkers 直接消费的 deprecated 快路径
    deprecated_name_uses: Vec<usize>,
    deprecated_member_uses: Vec<LuaSyntaxId>,
}

enum NameResolution {
    Local(SemanticId),
    Global(SemanticId),
    Unresolved,
}
```

`FileSemanticIndex` 在构建时一次性完成：

- 每个 `NameExpr` 的 local/global 判定；
- 每个 `IndexExpr` 的常规 member 解析；
- 无法确定身份或需要类型推断的 member use 标记为 `needs_slow_path`，不在此阶段强行解析。

### 4.3 SemanticModel

`resolve_member` 变成：

```rust
fn resolve_member(&self, expr) -> Option<ResolvedMember> {
    // 1. 优先使用 FileSemanticIndex 的预计算结果
    if let Some(resolved) = self.file_semantic_index().member_use(expr) {
        return Some(resolved);
    }

    // 2. 只有真正需要推断的表达式才走慢路径
    self.resolve_member_slow_path(expr)
}
```

这样：

- 简单 `a.b` / `M.x` / `Class.field` 等不再重复推断；
- 多次 checker 遍历同一文件时，只查一次表；
- 慢路径只处理少数动态/复杂表达式。

## 5. 查询 API 设计

### 5.1 WorkspaceIndex 查询

```rust
impl WorkspaceIndex {
    fn global_decl(&self, name: &str) -> Option<SemanticId>;
    fn global_type(&self, name: &str) -> Option<SemanticId>;
    fn runtime_decls(&self, name: &str) -> &[SemanticId];

    fn members(&self, owner: &SemanticId) -> &[MemberRef];
    fn members_named(&self, owner: &SemanticId, name: &str) -> &[MemberRef];

    fn type_members_named(&self, type_id: &SemanticId, name: &str) -> &[MemberRef];

    fn deprecated_members_named(&self, owner: &SemanticId, name: &str) -> &[MemberRef];
}
```

### 5.2 FileSemanticIndex 查询

```rust
impl FileSemanticIndex {
    fn resolve_name_use(&self, index: usize) -> NameResolution;
    fn resolve_member_use(&self, syntax: LuaSyntaxId) -> Option<&ResolvedMember>;
    fn deprecated_member_uses(&self) -> &[LuaSyntaxId];
}
```

## 6. 更新模型

保持现有 shard 思想，但 shard 内容从“数组”升级为“哈希片段”。

### 当前模型

```text
文件变更
  -> 重算 file_facts
  -> 重算 export_shard / reference_shard
  -> workspace index 重新 merge 数组
```

### 新模型

```text
文件变更
  -> 重算 file_facts
  -> 重算 FileSemanticIndex（per-file）
  -> 重算对应 shard 的哈希片段
  -> WorkspaceIndex 只 merge 该 shard 的 HashMap 片段
```

理想情况下，workspace index 的 merge 结果也可以被 salsa memoize：

- shard 不变 -> workspace index 不重算；
- shard 变 -> 只把变化的 key 插入/删除/替换到总表。

## 6.1 当前进度

- [x] Phase 1（部分）：`WorkspaceMemberIndex` 增加 `by_owner_name`
- [x] Phase 1：`WorkspaceDeclIndex` 增加 `runtime_by_name` / `runtime_by_file_name` / `type_def_by_id`
- [x] Phase 1：`resolve_owner_set` 不再线性扫描 `runtime_values` / `type_def_files`
- [x] Phase 1：`members_of_owner_named` 使用 `by_owner_name` 直查
- [x] Phase 2（部分）：`FileReferences` 增加 `name_use_resolution` / `member_use_to_member`
- [x] Phase 2：`resolve_member` fast path 覆盖 Name-rooted owner 与安全的 local runtime member
- [x] Phase 2：local fast path 排除 `---@type` / Param(`self`) / same-name doc member 等冲突场景
- [x] Phase 2（部分）：`DeprecatedChecker` 的 NameUse 路径使用 `FileReferences.name_use_resolution`
- [x] Phase 2（后续）：FileSemanticIndex 已覆盖简单 `---@type` local / `---@param` class member；
  冲突场景（同名 runtime member、nullable/generic/union 注解）仍安全地走慢路径
- [x] Phase 3（部分）：resolve_member 慢路径收敛
  - 慢路径中 3 / 3.5 / inherited fallback 只计算一次 `prefix_type_for_member_resolution`
  - 成员/继承展开增加 `MAX_MEMBER_INHERITANCE_DEPTH = 16` 深度上限
- [ ] Phase 3（后续）：profile 验证慢路径占比，继续收敛剩余场景
- [ ] Phase 4：整体接入 checker

## 7. 实施阶段

### Phase 1：把 WorkspaceIndex 的数组查询改成哈希查询

- `WorkspaceMemberIndex` 增加 `by_owner_name`
- `WorkspaceDeclIndex` 增加 `runtime_by_name`
- `resolve_owner_set` 不再 `runtime_values.iter().filter()`
- `members_of_owner_named` 直接查哈希索引

### Phase 2：建立 FileSemanticIndex

- 在现有 `file_references` 基础上扩展
- 保存 `NameResolution` 与 `ResolvedMember`
- 让 `SemanticModel::resolve_member` 优先查询它
- 让 deprecated checker 切换到 `FileSemanticIndex.deprecated_*`

### Phase 3：慢路径收敛

- 明确哪些表达式必须走 `resolve_member_slow_path`
- 给慢路径加收敛限制：
  - 不解析无法推断的 owner；
  - 不展开过深继承；
  - 不为同一表达式重复推断；
- 通过 profile 验证慢路径占比足够低

### Phase 4：整体接入 checker

- `DeprecatedChecker`
- `UndefinedFieldChecker`
- `AccessInvisibleChecker`
- `NeedCheckNilChecker`
- 后续所有 member 相关 checker

## 8. 风险与注意点

- `MemberRef` 目前只有 `file_id / id / name`，如果 `ResolvedMember` 需要更多字段，需要定义轻量跨文件表示；
- 类型成员索引如果直接展开继承，要小心循环继承 / 菱形继承 / 泛型父类；
- 建议类型成员索引保存“定义来源”，泛型实例化仍由 SemanticModel 完成；
- `FileSemanticIndex` 不能试图解析所有表达式，否则会重蹈前向 flow 预计算“全量预计算爆炸”的覆辙；
- 必须保持“只缓存有把握的结果，无法收敛的快速跳过”。

## 9. 成功指标

- `resolve_member` 常规路径不再进入多阶段 fallback；
- 对同一文件的多次 checker 遍历不重复解析同一 member use；
- DeprecatedChecker / UndefinedField / AccessInvisible 等明显下降；
- 编辑文件后增量更新时间与文件大小成正比，不与整个 workspace 大小成正比；
- 单文件大文件诊断时间进一步下降。

# Semantic Model 去 Db 化重构计划

## 目标

- 彻底移除 `semantic` 对 `DbIndex` 的直接依赖，不保留语义层兼容桥。
- 让 `SemanticModel` 只依赖 `LuaCompilation`、`SalsaSummaryHost` 和 parser AST，成为纯宿主驱动的语义入口。
- 把当前散落在 `db_index` 里的“文件/模块/类型/成员/签名/引用”读取责任，分别迁到 `compilation`、`summary_builder` 和独立 query/helper 层。
- 最终删除 `SemanticModel::get_db()`、`DiagnosticContext::get_db()` 以及 `semantic::*` 中所有以 `&DbIndex` 为主入口的 helper。

## 当前问题

从当前代码面看，`semantic` 对 `DbIndex` 的依赖不是局部问题，而是整层设计仍把 db 当成宿主：

- 宿主类依赖：`SemanticModel` 自己持有 `compilation`，但文档、uri、syntax tree、parse error、workspace 仍回落到 `DbIndex`。
- 类型系统依赖：`type_queries`、`type_check`、`generic`、`member`、`infer_*` 大量直接读 `type_index/member_index/signature_index`。
- 语义解析依赖：`semantic_info`、`decl`、`reference` 还在直接读 legacy decl/member/signature 结构。
- 诊断泄漏：大量 checker 通过 `semantic_model.get_db()` 或 `context.get_db()` 直接访问 db，绕过 `SemanticModel` / `LuaCompilation`。

这意味着如果继续按“某个 helper 改一下”的方式推进，会一直留下新的回退路径，最终无法真正删掉 db。

## 目标架构

语义宿主分三层：

1. `LuaCompilation`
   负责文件、模块、workspace、类型声明、成员聚合、类型 cache、签名、引用等稳定宿主能力。

2. `SalsaSummaryHost`
   负责 syntax-first summary、file/target semantic summary、program-point/type query、doc/module/property/lexical/flow/semantic graph/solver 等查询。

3. `semantic`
   只保留“语义编排和消费逻辑”：表达式推断、类型比较、泛型实例化、成员查找、语义 decl 解析、诊断辅助。
   `semantic` 内禁止直接触碰 `DbIndex`。

## 迁移原则

- 不新增过渡兼容层；只允许把宿主能力上提到 `LuaCompilation` / `summary facade`，然后立刻迁消费者。
- 优先迁“宿主读取接口”，再迁“语义算法内部逻辑”。先切断入口，再缩小内部 db 面。
- 优先迁稳定读路径：workspace、document、syntax tree、type decl、signature、member 聚合、reference。
- 任何新语义逻辑不得再引入 `get_db()` 新调用点。
- 每一阶段结束都要满足：新增消费者不再需要 `DbIndex`，并能删掉至少一批旧 helper 或旧 public API。

## 分阶段计划

### 阶段 0：建立宿主边界

目标：让 `SemanticModel` / `DiagnosticContext` 的基础读取都走 `LuaCompilation`。

实施项：

- 在 `LuaCompilation` 补齐宿主 API：document、uri->file、root、parse error、workspace、signature、members、reference 等。
- `SemanticModel` 的基础入口改走 `LuaCompilation`，不再自己读 vfs/workspace。
- `DiagnosticContext` 已有的 `find_type_decl/get_type_decl/get_decl_type/get_member_type/get_super_types` 继续统一走 `LuaCompilation`，补齐 `get_signature/get_members`。
- 明确把 `SemanticModel::get_db()` / `DiagnosticContext::get_db()` 标记为待删除 API，禁止新增调用点。

退出条件：

- `semantic/mod.rs` 的宿主读取不再直接用 `db.get_vfs()/resolve_workspace_id/get_signature_index/get_member_index`。
- `diagnostic/checker/mod.rs` 不再为已有 helper 读取签名/成员而直接下钻 db。

### 阶段 1：workspace/type decl 全量迁出

目标：类型声明可见性、workspace 归属、super type 查找全部走 `compilation::module/types`。

实施项：

- 把 `semantic_file_workspace_id/semantic_find_type_decl/semantic_get_type_decl` 改成基于 `LuaCompilation`。
- 清理 `visibility`、`semantic_info`、`infer_doc_type`、`member::find_members`、diagnostic checker 中对这些 helper 的 db 入口。
- 补齐 `LuaCompilation` 上缺失的 `find_type_decls/get_visible_type_decls/get_generic_params` 等 API，消掉 `db.get_type_index()` 直连。

退出条件：

- `semantic` 顶层不再有“按 file_id 从 db 取 workspace/type decl”的 helper。
- `visibility` 与 `semantic_info` 不再需要 `DbIndex` 参与 type decl lookup。

### 阶段 2：signature/member/reference 宿主收口

目标：签名、成员、引用的稳定读取全部从 `DbIndex` 挪到 `LuaCompilation`。

实施项：

- 给 `LuaCompilation` 补齐 `get_signature/get_members/get_member/get_member_owner/get_reference_index facade`。
- `semantic::member`、`semantic::reference`、`semantic_info`、diagnostic checker 改走 compilation。
- 收口 `SemanticModel::get_signature`、`signature_id_of_decl`、成员 owner 推断相关逻辑。

退出条件：

- `semantic/member`、`semantic/reference`、checker 不再直接读 `member_index/signature_index/reference_index`。

### 阶段 3：infer/type_check/generic 内核去 db

目标：把剩余算法型模块从 `&DbIndex` 参数改为新的宿主上下文。

实施项：

- 定义语义宿主上下文，直接持有 `&LuaCompilation` 与必要缓存，而不是 `&DbIndex`。
- 迁 `infer_name/infer_index/infer_call/infer_binary/infer_table/narrow/type_check/generic` 参数签名。
- 把所有 `TypeOps::*`、`expand_type/get_real_type/instantiate_type_generic/check_type_compact` 的 db 读取改为 compilation/type helper。

退出条件：

- `semantic/infer`、`semantic/type_check`、`semantic/generic` 不再以 `&DbIndex` 为主参数。

### 阶段 4：删除旧 API 和 db 入口

目标：彻底删掉 semantic 层残留的 db 兼容口。

实施项：

- 删除 `SemanticModel::get_db()`、`DiagnosticContext::get_db()`。
- 删除 semantic 内所有 `fn ...(db: &DbIndex, ...)` 的旧 helper 或改为 compilation/context 版本。
- 清理因为旧接口保留而存在的 fallback/compat 代码路径。

退出条件：

- `src/semantic/**` 中不再出现 `DbIndex` 类型参数或 `get_*_index()` 直连。
- 诊断 checker 不再通过 semantic/context 获取 db。

## 实施顺序

建议按下面顺序提交，避免范围过大：

1. 宿主 API 收口：`LuaCompilation` + `SemanticModel` + `DiagnosticContext`
2. workspace/type decl 迁移：`visibility`、`semantic_info`、`infer_doc_type`
3. signature/member/reference 迁移
4. infer/type_check/generic 参数面改造
5. 删除 `get_db()` 和旧 helper

## 风险与控制

- 风险：`LuaTypeDecl`、`LuaSignature`、`LuaMember` 这些返回值当前仍依赖 legacy db 存储。
  控制：先把“读取入口”集中到 `LuaCompilation`，再决定哪些要转成 compilation-owned 结构，避免边迁边扩散。

- 风险：diagnostic checker 数量多，直接删除 `get_db()` 会一次性炸太大。
  控制：先通过 `DiagnosticContext` / `SemanticModel` 补 host API，把调用点机械迁走，再删 `get_db()`。

- 风险：infer/type_check/generic 的函数签名巨大，重命名容易造成大面积漂移。
  控制：按模块分批迁，且每批以 `cargo check -p emmylua_code_analysis` 收口。

## 本轮先做什么

本轮开始实施阶段 0：

- 在 `LuaCompilation` 补基础宿主 API。
- 让 `SemanticModel` 的 document/root/parse/workspace/signature 先改走 `LuaCompilation`。
- 让 `DiagnosticContext` 的 `get_signature/get_members` 也改走 `LuaCompilation`。

完成这一步后，再继续推进阶段 1，把 `semantic_file_workspace_id` 和 type-decl helper 从 db 入口切到 compilation。

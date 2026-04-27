# Summary Builder 单文件语义统一推进文档

## 文档目标

这份文档替代以下三份已经发生分叉的旧文档：

1. `summary_builder_semantic_solver_architecture_CN.md`
2. `summary_builder_semantic_solver_checkpoint_CN.md`
3. `type_system_salsa_architecture_CN.md`

它只保留一条当前有效的路线，回答四个问题：

1. 当前仓库里已经稳定存在什么。
2. 当前真正的主矛盾是什么。
3. 接下来应该按什么顺序推进。
4. 每一阶段的退出条件和回归基线是什么。

## 一句话结论

当前项目已经不是“缺 graph / 缺 SCC / 缺 solver 宿主”的阶段。

当前真正的问题是：

1. 单文件语义已经有两条都很强的主线：`type_system` 的 program-point query 和 `semantic_solver` 的 solver-owned summary。
2. 这两条线都已经能产出稳定结果，但还没有完全收敛成同一套“权威语义层”。
3. 高层 consumer 仍大量停留在旧 `DbIndex + analyzer` 语义上。

因此当前最优先的方向不是继续扩 facade，也不是立刻做跨文件 graph host，而是：

1. 先把单文件权威语义层收口。
2. 再把高层 consumer 分批迁到这条新主路径。
3. 最后再进入跨文件宿主。

## 2026-04-21 当前分支核对结论

结合当前代码，短期内最值得继续推进的顺序需要再收紧一点：

1. 先继续消掉 query/solver 内部对同一类 candidate 证据的重复归约逻辑，优先统一到共享 candidate-shell helper。
2. 再处理还残留的高层 consumer 直读旧 index/旧 analyzer 语义入口。
3. 最后再进入 signature/doc type 摘要继续扩面，避免新能力继续落在分叉层上。

当前代码里已经可以确认：

1. `signature_summary / decl_summary / member_summary / resolved_type` 这些 summary-first 读口仍然稳定存在。
2. `DiagnosticContext` 和 `SemanticModel` 已经有一层 shared lookup/helper，可以继续承接 consumer 收口。
3. `semantic_solver` 里虽然已经有共享 candidate-shell helper，但 `for_range` 的 name/member source 直到本轮之前还在手工展开 candidate 列表，属于典型的“规则已经收口，调用点还没收口”。

因此本轮先落一小步：

1. 把 `for_range` 的 name/member source value shell 归约统一改成复用共享 candidate-shell helper。
2. 补针对 `named_type_names` 证据的回归，保证这条路径不再因为手工逻辑漏判 `Resolved`。

## 最新进度（本轮统一后）

### 已完成

1. `return_flow` 已从旧 analyzer 宿主迁到 `compilation/return_flow.rs`，`compilation/analyzer/lua/func_body.rs` 当前只保留兼容 re-export。
2. `semantic/mod.rs` 已接通 summary-first 的 require/export、call return、closure return、signature doc return 等高频读取口。
3. typed constructor table field closure 的 expected type 已打通合法 consumer 链：
	`local decl summary value shell -> declared named type -> LuaCompilation::find_type_merged_member -> summary property/doc type`。
4. `---@type` 不再作为 type owner 扩展来源；变量扩展仍只允许来自 `@class/@enum`。
5. `fun(...)` 没有显式 return items 的 summary function lowering 已按 `nil` 返回处理，`ReturnTypeMismatch` 与 `RedundantReturnValue` 相关关键回归重新一致。
6. semantic、visibility 与一部分 diagnostic checker 的 workspace/type lookup 已开始统一到 shared fallback helper：主工作区文件缺少 module 记录时，统一回退到 `WorkspaceId::MAIN`，避免 named type lowering、type def lookup、namespace member lookup、visibility internal check、duplicate type / circle class / type access modifier / generic string template lookup 在 consumer 层各自掉空。
7. “按当前文件语义查 type decl” 已开始上提为共享入口：semantic 侧有 `semantic_find_type_decl` / `SemanticModel::find_type_decl`，diagnostic 侧有 `DiagnosticContext::find_type_decl` / `get_visible_type_decls_by_full_name`，后续同类 consumer 不再需要自己拼 `type_index + workspace`。
8. 一部分按 id 的 type-index 读取也开始收口到 helper：semantic 侧已有 `semantic_get_type_decl` / `SemanticModel::get_type_decl`，diagnostic 侧已有 `DiagnosticContext::get_type_decl` / `get_super_types` / `get_file_type_decls`，开始替代 checker 内部直接访问 `type_index` 的细节。
9. diagnostic 侧的 signature 读取也开始上提为共享入口：semantic 侧已有 `SemanticModel::get_signature` / `signature_id_of_decl`，diagnostic 侧已有 `DiagnosticContext::get_signature`，`await_in_sync`、`undefined_doc_param`、`syntax_error`、`check_param_count`、`discard_returns` 等路径已切过去。
10. program-point 的 table-field property candidate 已补齐 `signature_offset`，不再落后于 decl/member/property summary 路径；当前已经有对应 program-point 回归覆盖 property closure candidate 的 signature 元数据存在性。
11. diagnostic 侧的 member/type-cache 读取也开始继续收口到 context helper：`DiagnosticContext` 已新增 `get_decl_type` / `get_member_type` / `get_members`，`missing_fields`、`enum_value_mismatch`、`assign_type_mismatch` 等 checker 已切走一批直接 `member_index + type_cache` 访问。
12. program-point 的 property candidate 现在不只验证 metadata 存在，还已有多返回 slot 精度回归：`{ pair() }` 这类 property/program-point 路径已经能稳定命中第二返回槽位，而不是只保留 `signature_offset`。
13. `duplicate_field` 也已切到 `DiagnosticContext::get_type_decl` / `get_members`，当前 diagnostic 里 direct `member_index/type_cache/signature_index` 访问基本只剩 helper 本身。
14. semantic_solver 侧新增了共享的 candidate-shell 归约入口，可直接从 `SalsaTypeCandidateSummary` 列表构建 value shell；这让 member/property/program-point 候选后续能共用同一套 shell 归约逻辑，而不必继续只依赖 member 专用 helper。
15. `for_range` 的 name/member source value shell 归约已切到共享 candidate-shell helper，并新增 `named_type_names` 证据回归，避免这条路径继续保留一份行为略旧的手工判定逻辑。
16. semantic_solver 内部的 call explain fallback resolve-state 判定也已收口到共享 helper；`signature return`、`for_range call source`、`call slot fallback` 三处不再各自维护一份略有偏差的状态规则，并新增回归锁定“candidate-only 仍为 Partial、return rows 即使没有 node offset 也算 Resolved”。
17. `call explain 是否 resolved` 的基础语义已经上提回 `query/signature.rs` 宿主；`signature return` 与 `semantic_solver` 现在复用同一个 helper，而不是继续由 solver 私有定义这条规则。
18. `signature return` 的 name/member candidate-state 判定已切到共享 candidate-shell helper，不再把“只要 candidates 非空”直接当成 Resolved；`named_type_names` 现在可稳定判为 Resolved，而只有 call-backed candidate 的路径会继续保持 Partial。
19. 已新增 `signature return` 对称回归，分别覆盖 `named_type_names -> Resolved` 与 `candidate-only call-backed name return -> Partial`，用于锁定 query 与 solver 这条规则不再回退到旧判定。
20. `for_range` 的 slot 元数据对称验证继续推进：已新增 member-source factory call 的回归，确认这条路径也能正确回填不同 iterator slot 的类型，不是当前 slot 消费面的缺口。
21. property query 的 slot 元数据对称验证已进一步补齐：除 type owner 外，`properties_for_decl_and_key` 与 `properties_for_member_and_key` 现在也都有 tail-call 多返回展开回归，`decl/member/type` 三类 owner 在 property query 层的 tail-slot 行为已经闭环。
22. 本轮继续尝试推进 `infer_expr_semantic_decl` 的高层 consumer 收口时，发现当前分支基线上 `test_semantic_model_member_summary_bridge` 已经失败；这说明 `semantic decl -> member bridge` 本身就是待处理问题，暂时不适合在这个入口上继续叠加新的行为改造或新增 failing 回归。
23. `semantic decl -> member bridge` 已恢复后，diagnostic 侧又完成了一小步只读 consumer 收口：`SemanticModel` 现在提供共享的 `get_decl(...)` 读口，`global_non_module`、`return_type_mismatch`、`call_non_callable` 三处高层 decl 读取已不再直接展开 `decl_index` 细节。
24. 上述三条 diagnostic 的定向测试在当前分支本身仍是红的，因此它们暂时不适合作为这批 helper 收口的回归基线；本轮可确认的稳定验证仍应优先依赖 `test_semantic_model_member_summary_bridge` 这类已修复的 summary-first 基线。
25. diagnostic 的只读 consumer 收口继续向前推了一步：`assign_type_mismatch` 与 `check_export` 里按 `decl_id` 回读声明的路径也已切到 `SemanticModel::get_decl(...)`，高层 checker 不再继续直接展开 `decl_index` 获取本地声明对象。
26. `assign_type_mismatch` 相关测试当前分支本身仍存在大量既有红测，因此这批 decl-read helper 收口同样不能依赖它作为回归基线；`check_export` 目前也没有命中的定向测试名，现阶段更稳的验证仍然是保住已转绿的 summary-first bridge 基线。
27. 为修复 `assign_type_mismatch` 里反复出现的“声明类型掉成 `Unknown`”，本轮已补一处更底层的类型绑定优先级修正：`LuaTypeIndex::bind_type(...)` 现在允许后续 `DocType` 覆盖已有的 `InferType`，避免 `---@type` / `---@class` 给出的声明类型被更早写入的 `Unknown/Nil` 永久挡住。
28. 同时也验证了 `assign_type_mismatch` 并不只是一个“读错 helper”的单点问题：该 checker 的 `IndexExpr` 左值读取已切到 `SemanticModel::infer_expr(...)`，但 `diagnostic::test::assign_type_mismatch_test::tests::test_1` 与 `test_valid_cases` 依旧维持红测，说明除了 decl/doc type 绑定之外，仍有更深一层的左值成员类型或 RHS 推断缺口尚未补齐。
29. 本轮开始按更激进的方向推进“type 读取权威入口上提到 compilation”：`LuaCompilation` 已新增一层 type compat facade，负责 `find/get type decl`、`super types`、`file/visible type decls`、`type cache`、`decl/member type`、`generic params` 等高层只读查询；这些 facade 目前优先使用 `CompilationTypeIndex` 做筛选/命中，再在内部薄兼容回落到旧 `LuaTypeIndex` 取 rich object。
30. 基于这层 compat facade，`SemanticModel` 与 `DiagnosticContext` 的高层 type helper 已切到 `LuaCompilation`，并顺手清掉了 diagnostic 目录里最后两个直接 `get_type_index()` 的 checker（`attribute_check`、`generic_constraint_mismatch`）；当前 `diagnostic/**/*.rs` 已不再直接消费旧 type index，后续剩余迁移重点转入 `semantic` 下的 infer/member/type_check/generic 内核。
31. `semantic/member/find_members.rs` 已新增只给 scoped 查询使用的 compilation-aware 兼容入口：`find_members_in_scope_with_compilation(...)` 与 `find_members_with_key_in_scope_with_compilation(...)`。当前仅 `SemanticModel::get_member_infos/get_member_info_with_key` 先切到这层，新路径内部优先经 `LuaCompilation` facade 读取 member type cache、type decl、super types，旧 `find_members(...)`/`find_members_with_key(...)` 及更深层内核调用暂不改签名，继续保留兼容壳。
32. `semantic/member/get_member_map.rs` 也已跟进相同模式：新增 `get_member_map_in_scope_with_compilation(...)`，并让 `SemanticModel::get_member_info_map(...)` 先切到 compilation-aware scoped 入口，member map 不再绕回旧 `find_members_in_scope(...)`。
33. `semantic/infer/infer_name.rs` 已新增 `infer_global_type_with_compilation(...)`；`semantic/infer/infer_index/mod.rs` 也补了一个很窄的 `infer_index_expr_with_compilation(...)` 入口，当前仅在 `Global/Namespace` 这两类高层 type lookup 上优先走 `LuaCompilation` facade，`SemanticModel::get_index_decl_type(...)` 已先切到这条新口，其余更深的 index 推断路径仍保持旧实现，等待下一轮继续内推。
34. `semantic/mod.rs` 里的 `SemanticModel` 已不再持有 `db: &DbIndex` 字段；当前模型对象只保留 `compilation + summary + cache`，所有仍需访问旧 index 的地方统一经 `self.compilation.get_db()` 转发。这样后续再看 `SemanticModel` 的残余旧语义访问时，入口已经被硬收敛到 compilation，不会再有第二条私有 `db` 直通路。
35. `SemanticModel::infer_expr(...)` 与 `SemanticModel::infer_bind_value_type(...)` 也已开始走 compilation-aware 包装入口：`semantic/infer/mod.rs` 新增 `infer_expr_with_compilation(...)` / `infer_bind_value_type_with_compilation(...)`，其中 `@as` 绑定读取、全局 name lookup、index lookup 会优先使用 `LuaCompilation` facade；其余尚未迁完的递归路径暂时仍回退旧实现，但默认高频入口已经不再直接以 db-only 推断作为第一选择。
36. `semantic/infer/infer_call/mod.rs` 的新路径也已继续收紧：`infer_call_expr_with_compilation(...)` 不再把 `LuaCompilation` 作为 `Option` 透传给共享 inner，而是与 legacy db-only 路径物理拆开；这让 compilation-first 的 call 推断入口语义上也变成硬约束，而不是“可选兼容”。
37. `semantic/infer/infer_table.rs` 里 table-field expected type 这条 compilation-first 路径已继续缩小旧 infer helper 的参与面：高频的 field member lookup 现在优先走 `find_members_with_key_in_scope_with_compilation(...)`，不再默认经 `infer_member_by_member_key / infer_member_by_operator` 读取旧 db 语义；旧 helper 仍保留在 legacy 路径中作为兼容壳。
38. `SemanticModel::infer_call_expr_func(...)` 这个高层 call consumer 也已跟进切到 compilation-aware prefix 推断；对外的 call function 解释入口不再先经 db-only `infer_expr(...)` 取得前缀类型，再进入后续 call resolver。
39. `semantic/infer/infer_table.rs` 的 compilation-first `infer_table_should_be_inner(...)` 里，`LocalStat` 与 `ReturnStat` 两个分支也已补上 compilation-aware helper，优先走 `get_compilation_type_cache(...)` 读取声明/返回点类型缓存；这两条高频上游分支不再在新路径里直接回落到旧 `db.get_type_index().get_type_cache(...)`。

### 正在进行

1. 继续收口 semantic/diagnostic consumer 中零散的旧 `DbIndex + analyzer` 主路径依赖。
2. 继续把高层 consumer 的 workspace/type/member 查询压到 shared helper 和 compilation facade，而不是在每个调用点单独兜底。
3. property/program-point 虽然已经有 slot 精度回归，但 semantic_solver 目前没有直接暴露 sequence property -> member summary 的等价读口；因此对称验证应继续优先放在 program-point 与 property query 这些真实可观测消费面，而不是勉强构造不存在的 solver target。
4. property query 这一层现在已经补齐 `decl/member/type` 三类 owner 的 tail-slot 回归，下一步不宜继续横向补同构测试，而应转去排查真正还在直接消费旧 index 的高层路径。
5. 下一轮更适合开始检查谁还在直接消费 member/property/program-point candidate 列表并自己判 shell/state，看看能否继续切到新的共享 candidate-shell helper。
6. `signature return` 这一类判定已经收口后，下一步就该继续排查 member/property/program-point candidate 的 slot 元数据对称消费，不再继续在 call-state 规则上投入过多迭代。
7. 如果 slot 元数据消费面已经基本对齐，再继续推进高层 consumer 从旧 `DbIndex + analyzer` 语义入口退场。
8. 进入下一批高层 consumer 收口前，应先确认 `semantic decl -> member bridge` 的基线状态；如果现有分支上连已有 bridge 回归都失败，就应先把它当成阻塞问题处理，而不是继续在其上叠加新重构。
9. `assign_type_mismatch` 这一支目前已确认至少分成两层问题：声明 doc type 绑定优先级已补，但 `namespace + typed local/member assignment` 与 `generic/class local init` 两组最小样例仍不通过；下一步应直接定位 `t.x = true` 这类场景里究竟是 `t` 的声明类型、`t.x` 的成员类型，还是 RHS 类型在 diagnostic 输入前掉成了 `Unknown/Any`。
10. 高层 diagnostic 线的 type 读取口已经基本收干净，下一步若继续沿“关闭对 db_index.type 的高层使用”推进，就应直接选择 `semantic` 内核中的一条子线分批处理，优先级建议是：`semantic_info/decl helper` -> `member/find_*` -> `infer_index + infer_name` -> `type_check/generic`，不要再回头在 diagnostic 外壳上做零碎清理。
11. `member/find_*` 当前已经完成第一刀最小切口：只让 `SemanticModel` 的 scoped member 查询先走 compilation-aware 入口。下一步不应立刻重写全部 `find_members` 系列旧入口，而应继续检查 `get_member_map_in_scope`、`infer_index`、`infer_name` 等仍直接依赖旧 member/type 读取的高层路径，按“新增 scoped compat 入口 -> 单个高层 consumer 切换”的方式继续向内推进。
12. `get_member_map_in_scope` 与 `get_index_decl_type` 这两条高层 consumer 现在都已经切到了 compat 新口；下一步更合适的延续点是把 `infer_index_expr_with_compilation(...)` 的 fast path 从目前的 `Global/Namespace` 扩到 `Ref/Def/Generic` 这类 custom type 路径，而不是一开始就改动 `infer_member_by_member_key` / `infer_member_by_operator` 的旧 public 入口签名。
13. `SemanticModel` 本体上的 `db` 私有字段已经删除后，下一步如果还要继续硬推迁移，优先级就不再是“删字段”，而是继续减少 `SemanticModel::db()` 这个薄转发下游的旧 helper 用量：先看 `infer_expr / infer_bind_value_type / semantic_info` 这些高频入口，逐步补 compilation facade 或临时空实现，再把对应调用点切走。
14. `infer_table` 已经证明：新路径如果还共享 `Option<&LuaCompilation>` inner，会持续把迁移目标稀释回“兼容模式”。后续 `infer_*_with_compilation(...)` 新入口应默认采用“必需 compilation 的 inner + 独立 legacy 入口”这一物理拆分模式，而不是再新增可选 compilation 参数。

### 近期执行顺序建议（供审阅）

1. 先盘点仍直接访问旧 `DbIndex`、`member_index`、`type_cache`、`signature_index` 的高层 consumer，按 semantic 和 diagnostic 两条线分别列清单。
2. 优先迁移“只读解释型” consumer：这类调用点改造成本低，最适合继续验证 shared helper 和 summary-first facade 的覆盖面。
3. 再处理还残留的 candidate-state 手工判定点，目标不是继续补测试数量，而是消掉最后几处 query/solver 外围的规则分叉。
4. 等高层 consumer 收口到一定程度后，再重新判断 semantic_solver 是否真的需要新增 property 级公开读口；在此之前，不建议为了测试对称性单独扩 solver facade。

### 当前剩余主矛盾

1. semantic 高层 consumer 仍有若干路径默认把旧 `DbIndex` 查询当第一真相源。
2. program-point query、solver summary、compilation facade 已经都能给出稳定结果，但还缺一套统一的 consumer 分层约束。
3. analyzer 虽然已经不再是推荐主路径，但兼容壳还在，高层切换仍需继续推进。

## 当前状态

### 1. 已经稳定存在的基础设施

当前仓库已经稳定具备：

1. syntax-first 的单文件 summary / facts。
2. `type_system` 下的声明候选、member 候选、program-point 候选与 narrowing。
3. semantic graph、SCC、worklist、component result 和最小 fixedpoint solver。
4. solver-owned 的 summary-first 公开读面：`signature / decl / member / for-range / module export / resolved doc type`。

这意味着“继续搭骨架”已经不是有效目标。

### 2. 当前单文件语义的真实分层

当前更准确的分层是：

1. `analysis/*`：稳定 syntax-first facts。
2. `query/type_system/*`：program-point 类型与局部语义解释。
3. `query/signature.rs`：call explain、signature return、overload return rows。
4. `query/semantic_graph.rs`：依赖图宿主。
5. `query/semantic_solver.rs`：component 级传播、fixedpoint 和 solver-owned summary。
6. `semantic/mod.rs`：对上层暴露新的 summary-first 读取口。

这里最重要的判断是：

1. `type_system` 不是前置设施，而是正在形成中的单文件权威 query 层。
2. `semantic_solver` 也不是试验性骨架，而是已经承担 component 聚合与对外 summary 的正式宿主。
3. 接下来必须决定两者如何协作，而不是继续各自扩面。

## 当前主矛盾

### 1. 不是“有没有 solver”

solver 侧已经具备：

1. component 调度。
2. predecessor 输入消费。
3. propagated / local / fixedpoint 分层。
4. decl/member/signature return/for-range/module export 的 summary-first 读面。

所以当前主矛盾不是“继续补一个 solver 入口”。

### 2. 不是“有没有 program-point query”

`type_system/program_point.rs` 已经覆盖：

1. local assignment 跟踪。
2. 基础 flow narrowing。
3. correlated overload row 过滤。
4. table shape 驱动的 member/index 行为。
5. 多返回调用在 decl assignment 上的 slot 精度。

所以当前主矛盾也不是“从零开始做 program-point query”。

### 3. 当前真正的主矛盾

当前真正要解决的是三件事：

1. 让 member/property 相关链路拥有和 decl 一样的多返回 slot 精度。
2. 让 solver 的 propagated/local/fixedpoint 不再只是字段分层，而是规则分层。
3. 让高层 consumer 默认读取 summary-first 单文件语义，而不是继续把旧 analyzer 当主实现。

## 统一推进顺序

## 阶段 1：收口单文件权威语义层

这是当前最高优先级阶段。

目标：

1. 让 `type_system` 的 program-point query 成为单文件局部类型真相源。
2. 让 `semantic_solver` 成为 component 聚合和公开 summary 真相源。
3. 明确两者的边界，不再重复扩相同能力。

### 阶段 1A：补齐 member/property 的多返回 slot 精度

当前已完成：

1. decl 声明式 initializer 已经保留 `value_result_index + source_call_syntax_id`。
2. solver 和 program-point 在 `local a, b = pair()` 上已经能按 slot 消费 call returns。

当前缺口：

1. member candidate 仍未统一携带 slot 元数据。
2. property candidate 仍未统一携带 slot 元数据。
3. 相关 alias / forwarded member 场景仍可能默认退回第 0 返回槽。

本阶段应先完成：

1. 把 slot 元数据提升到统一 candidate 层。
2. 让 member initializer 的 call path 按 slot 消费 call explain / signature return。
3. 补齐对应 semantic_solver 和 program-point 回归。

### 阶段 1B：收紧复杂 owner 下的 member/index program-point 规则

目标：

1. 收紧 alias、union、mapped owner、named type bridge 下的 owner -> member 候选桥接。
2. 明确保守回退边界，而不是继续依赖临时兜底。
3. 让复杂 owner 的 member/index 结果保持可解释、可测试。

这一步仍然以 query 为中心，不先动 consumer。

### 阶段 1C：固定“query 与 solver”的边界

统一约定：

1. 程序点局部类型结论优先来自 `type_system/program_point`。
2. component 聚合、cycle、predecessor 传播和公开 semantic summary 由 `semantic_solver` 负责。
3. 任何新单文件语义能力，先判断它是“局部程序点解释”还是“component 聚合传播”，不要双线重复实现。

阶段 1 的退出条件：

1. decl/member 的多返回 slot 精度对齐。
2. 复杂 owner 的 member/index program-point 行为边界固定。
3. 新增单文件语义需求能明确落到 query 或 solver 其中一边。

## 阶段 2：收紧 solver transfer 语义

这是当前第二优先级阶段。

目标：

1. 把 `propagated / local / fixedpoint` 从“结构分层”推进到“规则分层”。
2. 让 cycle component 的迭代依据不再只是通用 shell merge。
3. 为后续高层 consumer 迁移提供更稳定的 solver-owned summary。

应优先做：

1. 区分 doc-type、named-type、initializer-derived、call-return-derived 等证据来源。
2. 明确哪些证据可以传播、哪些只能本地消费。
3. 收紧 cycle transfer，而不是继续增加新的 façade。

阶段 2 的退出条件：

1. `propagated_value_shell` 与 `local_value_shell` 在行为上真正不同。
2. cycle 组件的 fixedpoint 不再主要依赖粗粒度 state/candidate union。
3. solver summary 的字段能够直接解释传播来源，而不是只暴露结果壳。

## 阶段 3：迁移高层 consumer

这是当前第三优先级阶段。

目标：

1. 让 `semantic/mod.rs` 暴露的 summary-first 入口成为默认高层读取路径。
2. 逐步削弱旧 `DbIndex + analyzer` 在高层语义中的主实现地位。
3. 保留 fallback，但让 fallback 真正退回兼容层。

建议顺序：

1. 先迁只读解释型 consumer。
2. 再迁 closure/signature 周边 consumer。
3. 最后再碰 `infer_expr_semantic_decl`、`type_check`、`diagnostic checker` 主路径。

当前进展：

1. closure/signature 周边已经实质进入这一阶段，typed constructor closure expected type、closure return、return count 的关键链路已切到 summary/completion facade 组合读取。
2. 本轮继续推进了 `semantic_info`、doc type lowering、namespace member lookup、visibility workspace check，以及一批 diagnostic checker 的 helper 统一，并开始把按名字与按 id 的 type-decl lookup、signature lookup 一起上提为共享入口。
3. `type_check`、大块 `infer_expr_semantic_decl`、旧 analyzer 主推断路径仍然应该后置，不在这一轮一起改。

理由：

1. 当前 `semantic/mod.rs` 已经提供 `signature_summary / decl_summary / member_summary / call_explain` 等入口。
2. 这层已经可以作为 consumer 切换桥接点。
3. 直接碰旧 infer/type_check/diagnostic 风险更高，应该后移。

阶段 3 的退出条件：

1. 高层只读语义查询优先经由 summary-first 入口。
2. 旧 infer/type_check/diagnostic 不再承担“单文件主语义解释器”的角色。
3. fallback 位置清晰、可枚举。

## 阶段 4：准备跨文件宿主

这是当前明确后置的阶段。

只有在阶段 1 到 3 足够稳定之后，才进入：

1. require/module export 跨文件桥接。
2. compilation 级 graph host。
3. dirty-region aware 的局部重算边界。

原因：

1. 如果单文件权威语义层还没有收口，跨文件宿主只会把分叉放大。
2. 如果高层 consumer 还没切主路径，跨文件 graph 也不会真正被消费。

## 当前不该优先做的事

当前不应优先投入：

1. 再扩一批 facade 名称或 query 名称。
2. 提前设计 compilation 级全图替换。
3. 直接删掉旧 analyzer。
4. 在 query 和 solver 两边同时实现同一条新语义规则。

## 回归基线

每次继续当前路线前，至少应跑：

1. `cargo test -p emmylua_code_analysis summary_builder -- --nocapture`
2. `cargo test -p emmylua_code_analysis semantic_solver -- --nocapture`
3. `cargo test -p emmylua_code_analysis signature_return -- --nocapture`

当阶段 1A 继续推进时，再额外锁定：

1. decl 多返回 slot 回归。
2. 新增 member 多返回 slot 回归。
3. 相关 program-point member 回归。

## 当前一句话行动建议

下一步直接做：

1. 继续把 semantic/diagnostic 高层 consumer 的 workspace/type/member lookup 统一到 shared helper + compilation facade。
2. 接着系统排查谁还在直接消费 member/property/program-point candidate 列表并自己判 shell/state，继续切到共享 candidate-shell helper。
3. 再顺手排查 semantic_solver 与上层 consumer 中剩余的 call explain state 直接判定点，优先改成共享 helper 或共享 facade。
4. 如果第 2、3 步已经没有成片残留，就转入 member/property candidate 的 slot 元数据统一携带。
5. 然后才进入 solver transfer 分层，避免 consumer 侧继续分叉。

这一步做完之后，再进入 solver transfer 分层，而不是先改跨文件。

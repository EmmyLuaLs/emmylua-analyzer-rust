# EmmyLua Salsa 语义迁移现状

> 更新日期：当前工作区
> 测试基线：`cargo test --workspace` 全绿
> 库内测试：`emmylua_code_analysis --lib` **1285 passed / 3 ignored**

---

## 一、总体原则

- 以语义正确性优先，**不做为了测试通过的 workaround**。
- 若某个旧测试与当前设计冲突（如 `test_issue_140_1`），暂时保留 `#[ignore]`，先补齐底层语义再评估。
- 当前活跃语义栈：
  - `salsa_builder`：facts / flow / query
  - `semantic_model`：flow / member / infer / type_check
  - `check`：诊断检查器
- 旧的 `db_index` / `semantic` / `compilation` 目录不再作为实现依据，只可用于理解旧行为。

---

## 二、已打通的核心语义

### 2.1 表达式与声明

- Salsa 表达式类型推断（VM 字节码式推断）。
- `NameExpr` / `IndexExpr` / `CallExpr` / `TableExpr` / 字面量 / 运算符。
- 局部、参数、全局、for 变量声明类型。
- 跨文件声明与类型定义解析。
- 签名结构：
  - `@param` / `@return`
  - 泛型参数、约束、默认值
  - `@overload` / `@return_overload`
  - `@return_cast`
  - `@class` / `@alias` / `@enum`

### 2.2 Flow / 控制流窄化

已实现：

- 赋值流：`x = value` / `t.x = value` / `self.x = value`
- 分支合并：单侧窄化不泄漏、多侧取并集
- 条件窄化：
  - `x == literal` / `x ~= literal`
  - `x == nil` / `x ~= nil`
  - `type(x) == '...'` / `type(x) ~= '...'` 基础类型过滤
  - `x == y` / `x ~= y`（另一侧为 Name / IndexExpr）
  - `return_cast` 与 `return_overload` 关联窄化
  - 成员判别：`x.kind == 'A'`
  - 动态成员判别：`obj[key] == "foo"`（key 已收窄为字符串常量）
  - 动态成员真值：`obj[key]` / `not obj[key]`
  - 静态成员真值：`obj.field` / `not obj.field`
  - 数组长度：
    - `#arr == n`
    - `#arr <= n` / `#arr < n` / `#arr >= n` / `#arr > n`
    - `not (#arr > n)`
    - `i64::MAX` 溢出反分支 → `Never`
  - 数字 for 循环内 `arr[i]` 非 nil
- `self` 已成为闭包作用域中的正式隐式参数，可参与 flow 窄化。
- 本地变量运行时成员赋值流优先于类声明字段。
- union 归一化：常量/窄类型被宽类型吸收（`"a" | string → string`）。

### 2.3 方法 / 成员

- 方法隐式 `self`：
  - 注册为 `DeclKind::Param`
  - `type_of_decl(self)` 返回方法 owner 类型
  - `self[op]` 动态 enum 索引
  - `self == self.parent` 流窄化
- 运行时成员流（`self.Abc = "a"` 后读取）。
- 类型成员 `@field`、继承、泛型实例成员。
- `pcall` / `xpcall` 回调转发与返回值。
- 方法点调用 receiver 参数对齐（`pcall(obj.method, obj, ...)`）。

### 2.4 类型检查

- 基础类型、常量、union、intersection、object、array、tuple、function、generic、conditional。
- 返回类型检查、参数类型检查、缺失字段、undefined field、assign mismatch、await/sync 等。

---

### 2.5 架构清理：移除 thread_local 与手工深度截断

- **彻底移除 `semantic_model` 中的 `thread_local!`**：
  - flow：删除 `FLOW_QUERY_DEPTH` / `TRACE_DEPTH` / `TD_COUNT` 调试计数器。
  - VM：闭包返回推断栈从线程局部状态迁移到 `SemanticModel` 实例字段。
- **flow 回溯全部显式迭代化**：
  - `trace_decl` / `trace_member`：线性 CFG 链改为 `loop + current`，不再递归进原生栈。
  - `branch_path_reachable` / `flow_node_reachable`：合并为显式工作栈可达性判断。
  - `antecedent_has_narrowing`：改为显式工作栈，`Multiple` 保持“所有分支均命中”语义。
  - 删除 `MAX_TRACE_DEPTH` 与 `PathState.depth`，主回溯路径不再有深度截断。

---

## 三、已解除 ignore 的代表性用例

| 用例 | 对应语义 |
|---|---|
| `test_issue_423` | 可空 `@param` 并入 nil |
| `test_issue_622` | 方法 `self` 动态 enum 索引 |
| `test_issue_627` | 成员判别窄化 |
| `test_discriminant_sibling_projection_preserves_missing_member_nil` | 缺失成员 union 诊断 |
| `test_assignment_table_rhs_keeps_multiple_narrowed_field_values` | 表字面量字段流 |
| `test_assignment_and_rhs_keeps_narrowed_index_on_second_operand` | 索引表达式流 |
| `test_index_expr_replay_keeps_literal_field_narrowing` | 成员表达式条件窄化 |
| `test_eq_uses_branch_narrowed_rhs_index_type` | `x == y.kind` |
| `test_eq_uses_branch_narrowed_dynamic_rhs_key` | 动态 key 右值 |
| `test_field_literal_eq_uses_branch_narrowed_dynamic_key` | 动态成员判别 |
| `test_field_literal_eq_uses_branch_narrowed_dynamic_key_index_dependency` | `keys[slot]` 常量传播 |
| `test_field_truthy_uses_branch_narrowed_dynamic_key` | 动态成员真值 |
| `test_stacked_dynamic_field_truthy_guards_build_semantic_model` | stacked 动态真值 |
| `test_stacked_same_field_truthiness_guards_build_semantic_model` | 静态成员真值 + 缺失成员视为 nil |
| `test_issue_524` | 数组长度/索引界内非 nil |
| `test_issue_1207_lesser_array_length_guards` | `#arr <=/<` 下界 |
| `test_issue_1207_empty_else_preserves_array_length_guard` | `not (#arr > n)` |
| `test_issue_1207_array_length_bound_does_not_overflow` | overflow 反分支不可达 |
| `test_self_1` | `self == self.parent` |
| `test_issue_630` | `self.Abc` 运行时成员赋值流 |
| `test_flow_assigned_call_type_guard_prefix_keeps_narrowing` | `TypeGuard<T>` 内置类型归一化 |
| `test_feature_generic_type_guard` | 类型守卫泛型绑定（VM 优先） |
| `test_unknown_type` | 未初始化局部读取按入“表达式级 Nil” |
| `test_issue_369` | 空对象 `{}` 在 TypeShell 层保留 `Object` |
| `test_feature_inherit_flow_from_const_local` | const local 布尔守卫一层继承 |
| `test_never_return_call_after_branch_statement_narrows_after_guard` | never/error 分支流 |
| `test_issue_877_never_return_call_narrows_after_guard` | never/error 分支流 |
| `test_reachable_assignment_over_never_value_contributes_to_merge` | never 赋值合并 |
| `test_false_call_condition_assignment_does_not_contribute_to_merge` | false 条件分支贡献 |
| `test_false_call_condition_doc_assignment_does_not_contribute_to_merge` | false 条件 doc 赋值 |
| `test_false_call_condition_missing_field_assignment_does_not_contribute_to_merge` | false 条件缺失字段 |

### 3.1 泛型能力近期进展

- **对象字面量/构造器泛型推断**
  - 恢复 8 个 `generic_infer_test` 对象字面量用例。
  - 新增条件类型 `infer P` 的 scoped lowering。
  - 新增 `expand_alias_generic` 与 `eval_conditionals`，支持对象、嵌套对象、函数字段、variadic 参数的结构匹配。

- **函数泛型推断进入诊断消费链路**
  - 恢复 `test_infer_type` / `test_return_generic` / `test_infer_parameters` / `test_infer_return_parameters` / `test_infer_new_constructor`。
  - 诊断参数检查现在会展开 simple/conditional generic alias。
  - `match_call_candidate` 在后续参数中代入先前 bindings，并延迟未绑定函数泛型的条件求值。
  - 检查器改用 `unify_call_bindings`，支持 generic union / string-template 绑定。
  - `has_new` 构造器条件支持类/字符串字面量构造器解析。

- **条件类型与分布式条件类型**
  - 非 `infer` 条件类型可以投影并求值。
  - 裸类型参数实例化为 union 时按成员分布式求值，正确丢弃 `never`。
  - 字面量条件保持精确：`"a" extends "b"` 为 false。
  - 泛型别名实例化阶段提前处理条件类型，避免丢失分布式语义。
  - 类型级 alias 的 rich 渲染会求值条件类型。

- **call operator / self 推断**
  - 新增 `call_operator_self_type`：支持类 call overload、union 可调用成员过滤、generic alias 透传、intersection 整体可调用。
  - 恢复 `Mod.Factory()`、`Callable|string`、`Box<Callable>`、`Callable & Extra` 等 self 返回。

- **缺失泛型实参归一化**
  - `---@type Box` 这类缺省泛型实参按 `default/constraint/unknown` 归一化为 `Generic` 参与 unify。
  - `unify` 现在会绑定 `Unknown` / `Never`，条件类型能在缺失/never 实参下正确求值。
  - 参数检查不再把 `Never` 当作“跳过检查”，使分布式条件产生的 `never` 能正确进入诊断。

---

## 四、当前剩余 ignored 主要缺口

### 4.1 已搁置

- `test_issue_140_1`
  - 当前语义只能从 `(Object|T)?` 推出 `Object|T`。
  - 测试期望 `T`，缺少可解释的守卫依据。
  - 按原则暂不强制，待确认原始 issue 语义。

### 4.2 Flow 系列

- stacked 系列：
  - 局部 call alias、type guard alias、stacked var equality 等。
  - 需要 const/local alias 与 return_cast 组合的更多 flow 路径。

- 循环后 flow：
  - `while true break`、`repeat`、`numeric for post_flow` 等。

- `test_type_narrow`
  - 与 `test_local_generics_in_global_scope` 存在泛型泄漏冲突，需先有原则性规则再恢复。

### 4.3 其他模块

- `semantic_model/type_check`、`generic_test`、`pcall_test` 等模块仍有少量 ignored 语义用例。
- 主要涉及：
  - 泛型调用/条件泛型
  - call overload self generic
  - 对象字面量构造器参数
  - tuple / unpack
  - setmetatable / metatable
  - 部分 migration 前的诊断检查用例

### 4.4 当前剩余 ignored（共 3 个，均为非语义迁移目标）

| 剩余项 | 状态 |
|---|---|
| `flow::test_issue_140_1` | 用户明确搁置，待确认原始语义 |
| `flow::test_type_narrow` | 用户确认不强制支持；当前与全局泛型泄漏规则冲突 |
| `tests::bench_clone_vs_query` | benchmark，不是语义测试 |

---

## 五、后续攻坚计划

当前语义迁移已基本完成：所有可恢复的 `#[ignore]` 语义测试均已恢复或经用户决策保留。

后续除非用户要求，否则不再为了 `test_issue_140_1` / `test_type_narrow` 引入 workaround。

如需继续，建议：
1. 重新评估 `test_issue_140_1` 的原始语义；
2. 为 `test_type_narrow` 设计新的作用域泛型规则（不破坏 `test_local_generics_in_global_scope`）；
3. benchmark 单独运行，不纳入语义迁移。

继续以“先语义、后测试”的顺序推进；不引入 workaround / thread_local / 深度截断 / 测试专用分支。

---

## 六、验证

```bash
cargo test --workspace
```

当前所有非 ignored 测试均应通过；新增语义必须保持 workspace 全绿。

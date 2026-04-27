# 类型系统迁移架构总结与后续计划

## 文档目标

这份文档服务于当前这一轮手动迁移工作，回答五个问题：

1. 当前类型系统正在从什么迁到什么。
2. 现在哪些层已经完成收口，哪些层仍然是阻塞点。
3. 修改时必须遵守的技术约束是什么。
4. 接下来应该按什么顺序继续手改。
5. 每一阶段应该用什么方式验证。

这不是历史回顾文档，也不是最终设计说明。它只描述 2026-04-23 当前分支上仍然有效的迁移现实。

## 一句话结论

当前迁移已经进入“去掉旧 `db_index.type` 作为事实来源”的中后段。

真正的目标不是把所有 `DbIndex` import 机械删光，而是把类型语义的权威读口收敛到：

1. `compilation/types/*` 作为权威类型数据层。
2. `LuaCompilation` 作为高层 façade 和未来 salsa 宿主入口。
3. `semantic/type_queries.rs` 作为过渡期查询边界。

当前最重要的剩余工作不是再改高层 checker 外壳，而是继续把 `types` 目录和 `semantic` 内核里残留的旧 type-index 语义依赖拔掉，最后再删除 `db_index` 兼容层。

## 当前目标架构

### 1. 分层

当前应以如下分层理解代码，而不是继续把 `db_index` 当中心：

1. `compilation/types/*`
   负责类型数据定义、类型运算、类型渲染等核心能力。
2. `compilation/mod.rs`
   负责聚合 façade，向上层暴露 type/member/decl/super-type 等读取口。
3. `semantic/type_queries.rs`
   负责过渡期查询 helper，把“需要查询宿主”的行为从纯数据对象中剥离出去。
4. `semantic/*`
   负责推断、类型检查、泛型实例化、成员查找等语义内核。
5. `db_index/*`
   当前只应继续退化为兼容层，而不应继续拥有新的权威实现。

### 2. 权责边界

应保持以下边界稳定：

1. 类型数据对象本身不直接查询 `DbIndex`。
2. `types` 目录内如果还需要查询宿主，应优先经 `semantic/type_queries.rs`，不要再直接展开 `get_type_index()` 细节。
3. 高层 consumer 应优先经 `LuaCompilation` 读取类型相关信息，而不是直接抓旧 index。
4. `db_index/type/mod.rs` 中的实现应持续退化成薄兼容壳，不能再承接新的真实语义。

## 当前已经完成的关键收口

### 1. 类型定义层已基本纯化

`compilation/types/type_decl.rs` 已回到纯数据层，之前挂在类型对象上的这类行为已经被剥离出去：

1. alias origin 查询。
2. enum field 类型查询。
3. super-type 收集。

这意味着 `LuaTypeDecl` 现在更接近真正的数据定义，而不是“带数据库方法的对象”。

### 2. `LuaCompilation` 已成为高层 type façade

`LuaCompilation` 现在已经承接一批高层类型读取口，包括：

1. type decl 查询。
2. super-type 查询与收集。
3. 一批 member/type cache/generic param 兼容 façade。

这一步的意义不是“再包一层”，而是把未来切到 salsa 宿主时的入口集中起来，避免高层 consumer 继续分散依赖旧 index。

### 3. 查询行为正在集中到 `semantic/type_queries.rs`

当前已经明确迁入这层的能力包括：

1. `get_type_decl(...)`
2. `get_generic_params(...)`
3. `get_type_cache(...)`
4. `get_signature(...)`
5. `get_alias_origin(...)`
6. `get_enum_field_type(...)`
7. `get_real_type(...)`
8. `has_userdata_super_type(...)`

这层的定位是过渡边界，不是终点。短期内它负责隔离旧查询细节，后续再逐步把宿主从 `DbIndex` 推到 `LuaCompilation`/salsa。

### 4. `types/type_ops` 已继续收紧

最近一轮之后，下列路径已不再直接展开旧 `type_index` 细节：

1. `union_type.rs` 的 real-type 和 signature 读取已改走 query helper。
2. `intersect_type.rs` 的 real-type 读取已改走 query helper。
3. `remove_type.rs` 的 type-decl、alias、userdata-super 判断已改走 query helper。

此外，旧 `db_index/type/mod.rs` 里的 `get_real_type(...)` 已退化为薄转发，不再是唯一真实实现。

## 当前仍然存在的主要阻塞

### 1. `types` 目录里仍有宿主型 `DbIndex` 依赖

这类依赖和“散落的旧索引直读”不同，但仍然是下一步要处理的重点：

1. `compilation/types/humanize_type.rs`
   当前大部分直接索引读取已外提，但整体宿主仍是 `DbIndex`。
2. `compilation/types/type_ops/mod.rs`
   `TypeOps::apply(...)` 仍以 `DbIndex` 作为操作宿主。

这两处代表的问题不再是“某一行还在 `get_type_index()`”，而是“整个 API 形状仍默认旧查询后端存在”。

### 2. `semantic` 内核仍有深层 db-bound 路径

当前剩余更深的旧依赖主要还在：

1. `semantic/type_check/*`
2. `semantic/generic/instantiate_type/*`
3. `semantic/infer/*` 中尚未切完的递归路径
4. 少数 diagnostic/infer path 仍通过旧兼容函数拿 real type

这些路径说明高层 façade 已经在收口，但真正最难的一层仍是语义内核本体。

### 3. `db_index/type/mod.rs` 仍然存在完整兼容结构

当前 `LuaTypeIndex` 仍然保留：

1. type decl rich object 存储。
2. generic param 存储。
3. super-type 存储。
4. type cache 存储。
5. 各种按作用域查找类型的旧接口。

这说明“能删 compat”还没有到最后一步。现在能做的是继续把真实行为往外挪，让 compat 逐步失去实质逻辑。

## 技术约束

### 1. `LuaType` 大小约束不可破坏

必须持续满足：

1. `LuaType` 是不超过 16 字节的 enum。
2. 相关布局预算测试必须持续通过。

这意味着任何新 id、新引用、新包装方案，都不能随意把重量重新塞回 `LuaType` 里。

### 2. `LuaTypeDeclId` 的实现约束

当前约束是：

1. `LuaTypeDeclId` 使用 `ArcIntern<LuaTypeIdentifier>`。
2. 迁移不能退回胖 id 或额外 owner 指针方案。

这条约束直接关系到布局、clone 成本和跨层传递成本。

### 3. `types` 目录不接受新的中间兼容层

这条约束要明确写死：

1. 不要在 `compilation/types/*` 里再塞新的 `DbIndex` 适配对象。
2. 不要把查询行为重新挂回 `LuaTypeDecl`、`LuaType` 或其他类型对象方法上。
3. 如果必须查询，优先新增 helper/query，而不是新增“带数据库的类型包装器”。

### 4. compilation-first 要求是真约束，不是可选增强

已经证明，如果新入口继续把 `LuaCompilation` 当 `Option` 透传给共享 inner，迁移会退化成兼容模式。因此后续新增路径应优先采用：

1. compilation-aware 新入口。
2. 独立 legacy 入口。

而不是再做可选 compilation 参数。

## 现在修改时的判断准则

当你准备手改一个点时，优先问四个问题：

1. 这个行为究竟是数据定义，还是查询行为。
2. 如果它是查询行为，应该属于 `LuaCompilation` façade，还是过渡期 `type_queries` helper。
3. 这次修改会不会把 `types` 目录重新绑回旧 `type_index` 细节。
4. 这次修改是在削弱 compat，还是在加固 compat。

只有答案是“查询被继续外提，compat 被继续变薄”时，方向才是对的。

## 推荐的后续手改顺序

### 阶段 1：清理 `types` 目录最后的宿主依赖

优先级最高，原因是这里是类型层的边界。

建议顺序：

1. 处理 `compilation/types/type_ops/mod.rs`
   目标是让 `TypeOps::apply(...)` 不再把 `DbIndex` 作为默认宿主形状。
2. 处理 `compilation/types/humanize_type.rs`
   目标是把剩余宿主语义继续外提，至少让它不再直接表达“类型渲染必须绑定旧 db”。

退出条件：

1. `types` 目录中不再出现对旧 `type_index` 细节的直接读取。
2. `types` 目录中的 `DbIndex` 只剩非常薄的宿主传递，或已被更窄的 query host 替代。

### 阶段 2：继续推 `semantic` 内核

建议优先顺序：

1. `semantic/type_check/*`
2. `semantic/generic/instantiate_type/*`
3. `semantic/infer/*` 中仍直接依赖旧 type-index 的路径

原因：

1. 这些模块最深地消费 alias、super-type、generic、signature 等类型语义。
2. 只有它们收完，`db_index.type` 才会真正失去存在必要。

退出条件：

1. semantic 内核不再直接展开旧 `LuaTypeIndex` 细节。
2. 高层 consumer 的 compilation-first 路径不再频繁回退到 legacy helper。

### 阶段 3：删 compat 层

这一阶段不要过早开始。真正可以动手删 compat 的信号应是：

1. `db_index/type/mod.rs` 中只剩薄转发和少量存储壳。
2. `semantic` 与 `types` 目录的主要读写路径都已切到 `LuaCompilation` 或 query helper。
3. 删除 compat 不会让项目“变得更坏”，而只是移除已失效的旧宿主。

到这个阶段再开始删：

1. `db_index/type/mod.rs` 的旧 helper 实现。
2. `db_index/type/type_decl.rs`、`type_owner.rs` 一类纯 re-export compat。
3. `DbIndex` 上与 type 相关但已经没有真实消费者的接口。

## 关键文件地图

当前手改最值得盯住的文件：

1. `crates/emmylua_code_analysis/src/compilation/mod.rs`
   高层 type/member façade 聚合入口。
2. `crates/emmylua_code_analysis/src/compilation/types/type_decl.rs`
   权威类型声明数据定义。
3. `crates/emmylua_code_analysis/src/semantic/type_queries.rs`
   过渡期查询边界。
4. `crates/emmylua_code_analysis/src/compilation/types/type_ops/mod.rs`
   当前 `types` 目录宿主层剩余入口之一。
5. `crates/emmylua_code_analysis/src/compilation/types/humanize_type.rs`
   当前类型渲染宿主剩余入口之一。
6. `crates/emmylua_code_analysis/src/semantic/type_check/*`
   语义内核深层迁移重点。
7. `crates/emmylua_code_analysis/src/semantic/generic/instantiate_type/*`
   泛型实例化深层迁移重点。
8. `crates/emmylua_code_analysis/src/db_index/type/mod.rs`
   最终要退场的 compat 中心。

## 验证策略

当前已知最稳定的窄验证基线仍然是：

1. `cargo test -p emmylua_code_analysis test_type_layout_budget -- --nocapture`

这条基线的重要性在于：

1. 它直接保护 `LuaType` 布局预算。
2. 它在当前分支上是稳定绿线。
3. 很适合在类型 id、类型结构和类型 helper 重排后快速回归。

手改时建议采用如下节奏：

1. 先做一小刀结构性收口。
2. 立即跑窄验证。
3. 验证通过后再继续下一刀。

如果要扩大验证面，再按你当时触及的模块补更窄的 targeted test 或 compile check，不要一开始就做全仓大扫。

## 最后判断

当前迁移已经不再是“要不要做 compilation-first”的讨论阶段，而是“怎么把最后几处真正阻塞 compilation-first 的旧语义依赖拔掉”的执行阶段。

因此接下来的手改应坚持三条原则：

1. 继续把查询行为从类型数据层和 compat 层往外推。
2. 继续把高层 consumer 的入口收口到 `LuaCompilation` 和共享 helper。
3. 不要为了短期编译通过重新引入新的 `types` 内兼容层。

只要这三条不回退，`db_index.type` 的最终退出就是时间问题，而不是方向问题。

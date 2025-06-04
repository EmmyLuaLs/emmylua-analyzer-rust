// use super::{BlockId, CfgBuilder, ControlFlowGraph, EdgeKind, FlowContext};
// use emmylua_parser::{
//     LuaAssignStat, LuaAstNode, LuaBlock, LuaBreakStat, LuaCallExpr, LuaChunk, LuaDoStat, LuaExpr,
//     LuaForRangeStat, LuaForStat, LuaFuncStat, LuaIfStat, LuaLocalStat, LuaRepeatStat,
//     LuaReturnStat, LuaStat, LuaSyntaxKind, LuaWhileStat,
// };
// use rowan::{SyntaxNode, TextRange};

// /// 控制流分析器
// pub struct CfgAnalyzer {
//     builder: CfgBuilder,
// }

// impl CfgAnalyzer {
//     pub fn new() -> Self {
//         Self {
//             builder: CfgBuilder::new(),
//         }
//     }

//     /// 分析Lua代码块并生成控制流图
//     pub fn analyze_chunk(mut self, chunk: LuaChunk) -> ControlFlowGraph {
//         // 创建入口块
//         let entry_block = self.builder.create_and_set_current_block();
//         self.builder.cfg.set_entry_block(entry_block);

//         // 创建出口块
//         let exit_block = self.builder.create_block();
//         self.builder.cfg.set_exit_block(exit_block);

//         // 设置return目标为出口块
//         let context = FlowContext::new().with_return_target(exit_block);
//         self.builder.push_context(context);

//         // 分析主代码块
//         if let Some(block) = chunk.get_block() {
//             self.analyze_block(block);
//         }

//         // 如果当前块还有效，连接到出口块
//         if let Some(current) = self.builder.get_current_block() {
//             self.builder.add_unconditional_edge(exit_block);
//         }

//         self.builder.finish()
//     }

//     /// 分析代码块
//     fn analyze_block(&mut self, block: LuaBlock) {
//         for stat in block.get_stats() {
//             self.analyze_statement(stat);

//             // 如果当前块已经结束（比如遇到return/break），停止处理后续语句
//             if self.builder.get_current_block().is_none() {
//                 break;
//             }
//         }
//     }

//     /// 分析语句
//     fn analyze_statement(&mut self, stat: LuaStat) {
//         let node = stat.syntax().clone();

//         // 检查是否已处理过
//         if self.builder.is_processed(&node).is_some() {
//             return;
//         }

//         match stat {
//             LuaStat::IfStat(if_stat) => self.analyze_if_statement(if_stat),
//             LuaStat::WhileStat(while_stat) => self.analyze_while_statement(while_stat),
//             LuaStat::RepeatStat(repeat_stat) => self.analyze_repeat_statement(repeat_stat),
//             LuaStat::ForStat(for_stat) => self.analyze_for_statement(for_stat),
//             LuaStat::ForRangeStat(for_range_stat) => {
//                 self.analyze_for_range_statement(for_range_stat)
//             }
//             LuaStat::DoStat(do_stat) => self.analyze_do_statement(do_stat),
//             LuaStat::ReturnStat(return_stat) => self.analyze_return_statement(return_stat),
//             LuaStat::BreakStat(break_stat) => self.analyze_break_statement(break_stat),
//             LuaStat::FuncStat(func_stat) => self.analyze_function_statement(func_stat),
//             LuaStat::LocalStat(local_stat) => self.analyze_local_statement(local_stat),
//             LuaStat::AssignStat(assign_stat) => self.analyze_assign_statement(assign_stat),
//             LuaStat::ExprStat(expr_stat) => self.analyze_expr_statement(expr_stat),
//             _ => {
//                 // 普通语句，添加到当前块
//                 self.builder.add_statement_to_current_block(node);
//             }
//         }
//     }

//     /// 分析if语句
//     fn analyze_if_statement(&mut self, if_stat: LuaIfStat) {
//         // 创建条件块
//         let condition_block = self.builder.create_and_set_current_block();

//         // 将条件表达式添加到条件块
//         if let Some(condition) = if_stat.get_condition() {
//             self.builder
//                 .add_statement_to_current_block(condition.syntax().clone());
//         }

//         // 创建then块和else块
//         let then_block = self.builder.create_block();
//         let else_block = self.builder.create_block();
//         let merge_block = self.builder.create_block();

//         // 添加条件边
//         self.builder.add_conditional_edge(then_block, else_block);

//         // 分析then分支
//         self.builder.set_current_block(then_block);
//         if let Some(then_body) = if_stat.get_then_block() {
//             self.analyze_block(then_body);
//         }
//         let then_exit = self.builder.get_current_block();

//         // 分析else分支
//         self.builder.set_current_block(else_block);
//         if let Some(else_body) = if_stat.get_else_block() {
//             self.analyze_block(else_body);
//         } else {
//             // 处理elseif链
//             for elseif in if_stat.get_elseif_stats() {
//                 if let Some(elseif_condition) = elseif.get_condition() {
//                     self.builder
//                         .add_statement_to_current_block(elseif_condition.syntax().clone());
//                 }
//                 if let Some(elseif_body) = elseif.get_block() {
//                     self.analyze_block(elseif_body);
//                 }
//             }
//         }
//         let else_exit = self.builder.get_current_block();

//         // 合并分支
//         self.builder
//             .merge_blocks(vec![then_exit, else_exit], merge_block);
//     }

//     /// 分析while循环语句
//     fn analyze_while_statement(&mut self, while_stat: LuaWhileStat) {
//         let condition_block = self.builder.create_and_set_current_block();
//         let body_block = self.builder.create_block();
//         let exit_block = self.builder.create_block();

//         // 将条件表达式添加到条件块
//         if let Some(condition) = while_stat.get_condition() {
//             self.builder
//                 .add_statement_to_current_block(condition.syntax().clone());
//         }

//         // 添加条件边
//         self.builder.add_conditional_edge(body_block, exit_block);

//         // 设置循环上下文
//         let loop_context = self
//             .builder
//             .current_context()
//             .with_loop_targets(exit_block, condition_block);
//         self.builder.push_context(loop_context);

//         // 分析循环体
//         self.builder.set_current_block(body_block);
//         if let Some(body) = while_stat.get_block() {
//             self.analyze_block(body);
//         }

//         // 循环体结束后跳回条件块
//         if let Some(current) = self.builder.get_current_block() {
//             self.builder.add_unconditional_edge(condition_block);
//         }

//         // 恢复上下文
//         self.builder.pop_context();

//         // 设置当前块为退出块
//         self.builder.set_current_block(exit_block);
//     }

//     /// 分析repeat-until循环语句
//     fn analyze_repeat_statement(&mut self, repeat_stat: LuaRepeatStat) {
//         let body_block = self.builder.create_and_set_current_block();
//         let condition_block = self.builder.create_block();
//         let exit_block = self.builder.create_block();

//         // 设置循环上下文
//         let loop_context = self
//             .builder
//             .current_context()
//             .with_loop_targets(exit_block, condition_block);
//         self.builder.push_context(loop_context);

//         // 分析循环体
//         if let Some(body) = repeat_stat.get_block() {
//             self.analyze_block(body);
//         }

//         // 循环体结束后到条件块
//         if let Some(current) = self.builder.get_current_block() {
//             self.builder.add_unconditional_edge(condition_block);
//         }

//         // 分析条件
//         self.builder.set_current_block(condition_block);
//         if let Some(condition) = repeat_stat.get_condition() {
//             self.builder
//                 .add_statement_to_current_block(condition.syntax().clone());
//         }

//         // 条件为假时继续循环，为真时退出
//         self.builder.add_conditional_edge(exit_block, body_block);

//         // 恢复上下文
//         self.builder.pop_context();

//         // 设置当前块为退出块
//         self.builder.set_current_block(exit_block);
//     }

//     /// 分析for循环语句
//     fn analyze_for_statement(&mut self, for_stat: LuaForStat) {
//         let init_block = self.builder.create_and_set_current_block();
//         let condition_block = self.builder.create_block();
//         let body_block = self.builder.create_block();
//         let update_block = self.builder.create_block();
//         let exit_block = self.builder.create_block();

//         // 初始化
//         if let Some(init_expr) = for_stat.get_init_expr() {
//             self.builder
//                 .add_statement_to_current_block(init_expr.syntax().clone());
//         }
//         self.builder.add_unconditional_edge(condition_block);

//         // 条件检查
//         self.builder.set_current_block(condition_block);
//         if let Some(condition_expr) = for_stat.get_limit_expr() {
//             self.builder
//                 .add_statement_to_current_block(condition_expr.syntax().clone());
//         }
//         self.builder.add_conditional_edge(body_block, exit_block);

//         // 设置循环上下文
//         let loop_context = self
//             .builder
//             .current_context()
//             .with_loop_targets(exit_block, update_block);
//         self.builder.push_context(loop_context);

//         // 分析循环体
//         self.builder.set_current_block(body_block);
//         if let Some(body) = for_stat.get_block() {
//             self.analyze_block(body);
//         }

//         // 更新步长
//         if let Some(current) = self.builder.get_current_block() {
//             self.builder.add_unconditional_edge(update_block);
//         }
//         self.builder.set_current_block(update_block);
//         if let Some(step_expr) = for_stat.get_step_expr() {
//             self.builder
//                 .add_statement_to_current_block(step_expr.syntax().clone());
//         }
//         self.builder.add_unconditional_edge(condition_block);

//         // 恢复上下文
//         self.builder.pop_context();

//         // 设置当前块为退出块
//         self.builder.set_current_block(exit_block);
//     }

//     /// 分析for-in循环语句
//     fn analyze_for_range_statement(&mut self, for_range_stat: LuaForRangeStat) {
//         let init_block = self.builder.create_and_set_current_block();
//         let condition_block = self.builder.create_block();
//         let body_block = self.builder.create_block();
//         let exit_block = self.builder.create_block();

//         // 初始化迭代器
//         for expr in for_range_stat.get_expr_list() {
//             self.builder
//                 .add_statement_to_current_block(expr.syntax().clone());
//         }
//         self.builder.add_unconditional_edge(condition_block);

//         // 条件检查（迭代器是否有下一个值）
//         self.builder.set_current_block(condition_block);
//         self.builder.add_conditional_edge(body_block, exit_block);

//         // 设置循环上下文
//         let loop_context = self
//             .builder
//             .current_context()
//             .with_loop_targets(exit_block, condition_block);
//         self.builder.push_context(loop_context);

//         // 分析循环体
//         self.builder.set_current_block(body_block);
//         if let Some(body) = for_range_stat.get_block() {
//             self.analyze_block(body);
//         }

//         // 循环体结束后回到条件检查
//         if let Some(current) = self.builder.get_current_block() {
//             self.builder.add_unconditional_edge(condition_block);
//         }

//         // 恢复上下文
//         self.builder.pop_context();

//         // 设置当前块为退出块
//         self.builder.set_current_block(exit_block);
//     }

//     /// 分析do语句
//     fn analyze_do_statement(&mut self, do_stat: LuaDoStat) {
//         if let Some(block) = do_stat.get_block() {
//             self.analyze_block(block);
//         }
//     }

//     /// 分析return语句
//     fn analyze_return_statement(&mut self, return_stat: LuaReturnStat) {
//         // 将return语句添加到当前块
//         self.builder
//             .add_statement_to_current_block(return_stat.syntax().clone());

//         // 添加返回边
//         self.builder.add_return_edge();
//     }

//     /// 分析break语句
//     fn analyze_break_statement(&mut self, break_stat: LuaBreakStat) {
//         // 将break语句添加到当前块
//         self.builder
//             .add_statement_to_current_block(break_stat.syntax().clone());

//         // 添加break边
//         self.builder.add_break_edge();
//     }

//     /// 分析函数定义语句
//     fn analyze_function_statement(&mut self, func_stat: LuaFuncStat) {
//         // 函数定义本身添加到当前块
//         self.builder
//             .add_statement_to_current_block(func_stat.syntax().clone());

//         // 注意：函数体的CFG应该单独分析，这里暂时不处理
//     }

//     /// 分析局部变量声明语句
//     fn analyze_local_statement(&mut self, local_stat: LuaLocalStat) {
//         self.builder
//             .add_statement_to_current_block(local_stat.syntax().clone());
//     }

//     /// 分析赋值语句
//     fn analyze_assign_statement(&mut self, assign_stat: LuaAssignStat) {
//         self.builder
//             .add_statement_to_current_block(assign_stat.syntax().clone());
//     }

//     /// 分析表达式语句
//     fn analyze_expr_statement(&mut self, expr_stat: LuaExprStat) {
//         self.builder
//             .add_statement_to_current_block(expr_stat.syntax().clone());
//     }
// }

// impl Default for CfgAnalyzer {
//     fn default() -> Self {
//         Self::new()
//     }
// }

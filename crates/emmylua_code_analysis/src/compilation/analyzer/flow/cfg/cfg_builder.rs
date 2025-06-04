use super::{ControlFlowGraph, BasicBlock, BlockId, EdgeKind, FlowContext};
use emmylua_parser::{
    LuaStat, LuaSyntaxNode
};
use rowan::{SyntaxNode, TextRange};
use std::collections::HashMap;

#[derive(Debug)]
pub struct CfgBuilder {
    /// 控制流图实例
    pub cfg: ControlFlowGraph,
    /// 当前活跃的基本块
    pub current_block: Option<BlockId>,
    /// 流程上下文栈
    pub context_stack: Vec<FlowContext>,
    /// 已处理的节点，避免重复处理
    processed_nodes: HashMap<LuaSyntaxNode, BlockId>,
}

impl CfgBuilder {
    pub fn new() -> Self {
        Self {
            cfg: ControlFlowGraph::new(),
            current_block: None,
            context_stack: vec![FlowContext::new()],
            processed_nodes: HashMap::new(),
        }
    }
    
    /// 获取当前的流程上下文
    pub fn current_context(&self) -> &FlowContext {
        self.context_stack.last().unwrap()
    }
    
    /// 推入新的流程上下文
    pub fn push_context(&mut self, context: FlowContext) {
        self.context_stack.push(context);
    }
    
    /// 弹出流程上下文
    pub fn pop_context(&mut self) -> Option<FlowContext> {
        if self.context_stack.len() > 1 {
            self.context_stack.pop()
        } else {
            None
        }
    }
    
    /// 创建新的基本块并设为当前块
    pub fn create_and_set_current_block(&mut self) -> BlockId {
        let block_id = self.cfg.create_block();
        self.current_block = Some(block_id);
        block_id
    }
    
    /// 创建新的基本块但不设为当前块
    pub fn create_block(&mut self) -> BlockId {
        self.cfg.create_block()
    }
    
    /// 设置当前基本块
    pub fn set_current_block(&mut self, block_id: BlockId) {
        self.current_block = Some(block_id);
    }
    
    /// 获取当前基本块ID
    pub fn get_current_block(&self) -> Option<BlockId> {
        self.current_block
    }
    
    /// 向当前基本块添加语句
    pub fn add_statement_to_current_block(&mut self, stmt: LuaStat) {
        if let Some(current_id) = self.current_block {
            if let Some(block) = self.cfg.get_block_mut(current_id) {
                block.add_statement(stmt);
            }
        }
    }
    
    /// 添加无条件边并更新当前块
    pub fn add_unconditional_edge(&mut self, to: BlockId) {
        if let Some(from) = self.current_block {
            self.cfg.add_edge(from, to, EdgeKind::Unconditional);
        }
        self.current_block = Some(to);
    }
    
    /// 添加条件边
    pub fn add_conditional_edge(&mut self, to_true: BlockId, to_false: BlockId) {
        if let Some(from) = self.current_block {
            self.cfg.add_edge(from, to_true, EdgeKind::ConditionalTrue);
            self.cfg.add_edge(from, to_false, EdgeKind::ConditionalFalse);
        }
        // 条件分支后当前块变为None，需要明确设置
        self.current_block = None;
    }
    
    /// 添加break边
    pub fn add_break_edge(&mut self) {
        if let (Some(from), Some(break_target)) = (self.current_block, self.current_context().break_target) {
            self.cfg.add_edge(from, break_target, EdgeKind::Break);
        }
        self.current_block = None; // break后当前路径结束
    }
    
    /// 添加continue边
    pub fn add_continue_edge(&mut self) {
        if let (Some(from), Some(continue_target)) = (self.current_block, self.current_context().continue_target) {
            self.cfg.add_edge(from, continue_target, EdgeKind::Continue);
        }
        self.current_block = None; // continue后当前路径结束
    }
    
    /// 添加return边
    pub fn add_return_edge(&mut self) {
        if let (Some(from), Some(return_target)) = (self.current_block, self.current_context().return_target) {
            self.cfg.add_edge(from, return_target, EdgeKind::Return);
        }
        self.current_block = None; // return后当前路径结束
    }
    
    /// 合并多个基本块到一个目标块
    pub fn merge_blocks(&mut self, sources: Vec<Option<BlockId>>, target: BlockId) {
        for source in sources.into_iter().flatten() {
            self.cfg.add_edge(source, target, EdgeKind::Unconditional);
        }
        self.current_block = Some(target);
    }
    
    /// 标记节点已处理
    pub fn mark_processed(&mut self, node: LuaSyntaxNode, block_id: BlockId) {
        self.processed_nodes.insert(node, block_id);
    }
    
    /// 检查节点是否已处理
    pub fn is_processed(&self, node: &LuaSyntaxNode) -> Option<BlockId> {
        self.processed_nodes.get(node).copied()
    }
    
    /// 完成CFG构建
    pub fn finish(mut self) -> ControlFlowGraph {
        // 确保有入口和出口块
        if self.cfg.entry_block.is_none() && !self.cfg.blocks.is_empty() {
            let first_block = *self.cfg.blocks.keys().next().unwrap();
            self.cfg.set_entry_block(first_block);
        }
        
        if self.cfg.exit_block.is_none() {
            let exit_block = self.cfg.create_block();
            self.cfg.set_exit_block(exit_block);
        }
        
        self.cfg
    }
}

impl Default for CfgBuilder {
    fn default() -> Self {
        Self::new()
    }
}
mod cfg_analyzer;
mod cfg_builder;
mod cfg_visualizer;

#[cfg(test)]
mod tests;

use emmylua_parser::LuaStat;
use rowan::TextRange;
use std::collections::{HashMap, HashSet};

pub use cfg_builder::CfgBuilder;

#[allow(unused)]
pub use cfg_visualizer::{CfgStats, CfgVisualizer};

/// 控制流图的基本块ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub usize);

impl BlockId {
    pub fn new(id: usize) -> Self {
        Self(id)
    }

    pub fn as_usize(&self) -> usize {
        self.0
    }
}

/// 控制流图的边类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeKind {
    /// 无条件跳转
    Unconditional,
    /// 条件为真时的跳转
    ConditionalTrue,
    /// 条件为假时的跳转
    ConditionalFalse,
    /// break语句跳转
    Break,
    /// continue语句跳转
    Continue,
    /// return语句跳转
    Return,
    /// 异常跳转
    Exception,
}

/// 控制流图的边
#[derive(Debug, Clone)]
pub struct CfgEdge {
    pub from: BlockId,
    pub to: BlockId,
    pub kind: EdgeKind,
}

/// 控制流图的基本块
#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    /// 基本块包含的语句
    pub statements: Vec<LuaStat>,
    /// 基本块的文本范围
    pub range: Option<TextRange>,
    /// 前驱基本块
    pub predecessors: HashSet<BlockId>,
    /// 后继基本块
    pub successors: HashSet<BlockId>,
    /// 是否是入口块
    pub is_entry: bool,
    /// 是否是出口块
    pub is_exit: bool,
}

impl BasicBlock {
    pub fn new(id: BlockId) -> Self {
        Self {
            id,
            statements: Vec::new(),
            range: None,
            predecessors: HashSet::new(),
            successors: HashSet::new(),
            is_entry: false,
            is_exit: false,
        }
    }

    pub fn add_statement(&mut self, stmt: LuaStat) {
        self.statements.push(stmt);
    }

    pub fn set_range(&mut self, range: TextRange) {
        self.range = Some(range);
    }

    pub fn mark_as_entry(&mut self) {
        self.is_entry = true;
    }

    pub fn mark_as_exit(&mut self) {
        self.is_exit = true;
    }
}

/// 控制流图
#[derive(Debug)]
pub struct ControlFlowGraph {
    /// 所有基本块
    pub blocks: HashMap<BlockId, BasicBlock>,
    /// 所有边
    pub edges: Vec<CfgEdge>,
    /// 入口块ID
    pub entry_block: Option<BlockId>,
    /// 出口块ID
    pub exit_block: Option<BlockId>,
    /// 下一个可用的块ID
    next_block_id: usize,
}

impl ControlFlowGraph {
    pub fn new() -> Self {
        Self {
            blocks: HashMap::new(),
            edges: Vec::new(),
            entry_block: None,
            exit_block: None,
            next_block_id: 0,
        }
    }

    /// 创建新的基本块
    pub fn create_block(&mut self) -> BlockId {
        let id = BlockId::new(self.next_block_id);
        self.next_block_id += 1;

        let block = BasicBlock::new(id);
        self.blocks.insert(id, block);
        id
    }

    /// 获取基本块的可变引用
    pub fn get_block_mut(&mut self, id: BlockId) -> Option<&mut BasicBlock> {
        self.blocks.get_mut(&id)
    }

    /// 获取基本块的不可变引用
    pub fn get_block(&self, id: BlockId) -> Option<&BasicBlock> {
        self.blocks.get(&id)
    }

    /// 添加边
    pub fn add_edge(&mut self, from: BlockId, to: BlockId, kind: EdgeKind) {
        // 更新基本块的前驱和后继
        if let Some(from_block) = self.blocks.get_mut(&from) {
            from_block.successors.insert(to);
        }
        if let Some(to_block) = self.blocks.get_mut(&to) {
            to_block.predecessors.insert(from);
        }

        // 添加边
        self.edges.push(CfgEdge { from, to, kind });
    }

    /// 设置入口块
    pub fn set_entry_block(&mut self, id: BlockId) {
        self.entry_block = Some(id);
        if let Some(block) = self.blocks.get_mut(&id) {
            block.mark_as_entry();
        }
    }

    /// 设置出口块
    pub fn set_exit_block(&mut self, id: BlockId) {
        self.exit_block = Some(id);
        if let Some(block) = self.blocks.get_mut(&id) {
            block.mark_as_exit();
        }
    }

    /// 获取所有基本块的ID
    pub fn block_ids(&self) -> impl Iterator<Item = BlockId> + '_ {
        self.blocks.keys().copied()
    }

    /// 获取指定块的前驱
    pub fn predecessors(&self, id: BlockId) -> Option<&HashSet<BlockId>> {
        self.blocks.get(&id).map(|block| &block.predecessors)
    }

    /// 获取指定块的后继
    pub fn successors(&self, id: BlockId) -> Option<&HashSet<BlockId>> {
        self.blocks.get(&id).map(|block| &block.successors)
    }
}

impl Default for ControlFlowGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// 控制流上下文，用于跟踪循环和分支结构
#[derive(Debug, Clone)]
pub struct FlowContext {
    /// 当前的break目标
    pub break_target: Option<BlockId>,
    /// 当前的continue目标
    pub continue_target: Option<BlockId>,
    /// 当前的return目标
    pub return_target: Option<BlockId>,
    /// 嵌套深度
    pub depth: usize,
}

impl FlowContext {
    pub fn new() -> Self {
        Self {
            break_target: None,
            continue_target: None,
            return_target: None,
            depth: 0,
        }
    }

    pub fn with_loop_targets(&self, break_target: BlockId, continue_target: BlockId) -> Self {
        Self {
            break_target: Some(break_target),
            continue_target: Some(continue_target),
            return_target: self.return_target,
            depth: self.depth + 1,
        }
    }

    pub fn with_return_target(&self, return_target: BlockId) -> Self {
        Self {
            break_target: self.break_target,
            continue_target: self.continue_target,
            return_target: Some(return_target),
            depth: self.depth,
        }
    }
}

impl Default for FlowContext {
    fn default() -> Self {
        Self::new()
    }
}

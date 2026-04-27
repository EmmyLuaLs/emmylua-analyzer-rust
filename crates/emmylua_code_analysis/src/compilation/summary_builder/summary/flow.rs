use rowan::TextSize;
use smol_str::SmolStr;

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaFlowBlockOwnerKindSummary {
    Chunk,
    DoStat,
    IfStat,
    ElseIfClause,
    ElseClause,
    WhileStat,
    RepeatStat,
    ForStat,
    ForRangeStat,
    FuncStat,
    LocalFuncStat,
    ClosureExpr,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowBlockSummary {
    pub syntax_offset: TextSize,
    pub owner_kind: SalsaFlowBlockOwnerKindSummary,
    pub owner_offset: TextSize,
    pub parent_block_offset: Option<TextSize>,
    pub child_block_offsets: Vec<TextSize>,
    pub statement_offsets: Vec<TextSize>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaFlowStatementKindSummary {
    Linear,
    Do,
    Branch,
    Loop,
    Return,
    Break,
    Goto,
    Label,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowStatementSummary {
    pub syntax_offset: TextSize,
    pub block_offset: TextSize,
    pub kind: SalsaFlowStatementKindSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaFlowConditionKindSummary {
    Expr,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowConditionSummary {
    pub node_offset: u32,
    pub syntax_offset: TextSize,
    pub kind: SalsaFlowConditionKindSummary,
    pub left_condition_offset: Option<u32>,
    pub right_condition_offset: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaFlowBranchClauseKindSummary {
    If,
    ElseIf,
    Else,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowBranchClauseSummary {
    pub kind: SalsaFlowBranchClauseKindSummary,
    pub syntax_offset: TextSize,
    pub condition_expr_offset: Option<TextSize>,
    pub condition_node_offset: Option<u32>,
    pub block_offset: Option<TextSize>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowBranchSummary {
    pub syntax_offset: TextSize,
    pub clauses: Vec<SalsaFlowBranchClauseSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaFlowLoopKindSummary {
    While,
    Repeat,
    For,
    ForRange,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowLoopSummary {
    pub syntax_offset: TextSize,
    pub kind: SalsaFlowLoopKindSummary,
    pub condition_expr_offset: Option<TextSize>,
    pub condition_node_offset: Option<u32>,
    pub iter_expr_offsets: Vec<TextSize>,
    pub block_offset: Option<TextSize>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowReturnSummary {
    pub syntax_offset: TextSize,
    pub block_offset: Option<TextSize>,
    pub expr_offsets: Vec<TextSize>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowBreakSummary {
    pub syntax_offset: TextSize,
    pub block_offset: Option<TextSize>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowGotoSummary {
    pub syntax_offset: TextSize,
    pub block_offset: Option<TextSize>,
    pub label_name: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowLabelSummary {
    pub syntax_offset: TextSize,
    pub block_offset: Option<TextSize>,
    pub name: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowSummary {
    pub branch_count: usize,
    pub loop_count: usize,
    pub return_count: usize,
    pub block_count: usize,
    pub break_count: usize,
    pub goto_count: usize,
    pub label_count: usize,
    pub blocks: Vec<SalsaFlowBlockSummary>,
    pub statements: Vec<SalsaFlowStatementSummary>,
    pub conditions: Vec<SalsaFlowConditionSummary>,
    pub branches: Vec<SalsaFlowBranchSummary>,
    pub loops: Vec<SalsaFlowLoopSummary>,
    pub returns: Vec<SalsaFlowReturnSummary>,
    pub breaks: Vec<SalsaFlowBreakSummary>,
    pub gotos: Vec<SalsaFlowGotoSummary>,
    pub labels: Vec<SalsaFlowLabelSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowBranchLinkSummary {
    pub branch_offset: TextSize,
    pub entry_block_offset: Option<TextSize>,
    pub clause_block_offsets: Vec<TextSize>,
    pub exit_block_offset: Option<TextSize>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowLoopLinkSummary {
    pub loop_offset: TextSize,
    pub entry_block_offset: Option<TextSize>,
    pub body_block_offset: Option<TextSize>,
    pub continue_block_offset: Option<TextSize>,
    pub exit_block_offset: Option<TextSize>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowReturnLinkSummary {
    pub return_offset: TextSize,
    pub block_offset: Option<TextSize>,
    pub exit_root_block_offset: Option<TextSize>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowBreakLinkSummary {
    pub break_offset: TextSize,
    pub block_offset: Option<TextSize>,
    pub enclosing_loop_offset: Option<TextSize>,
    pub exit_block_offset: Option<TextSize>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowGotoLinkSummary {
    pub goto_offset: TextSize,
    pub block_offset: Option<TextSize>,
    pub target_label_offset: Option<TextSize>,
    pub target_block_offset: Option<TextSize>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaFlowTerminalEdgeKindSummary {
    Return,
    Break,
    Goto,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaFlowNodeRefSummary {
    Block(TextSize),
    Statement(TextSize),
    Condition(u32),
    Branch(TextSize),
    Loop(TextSize),
    Merge(TextSize),
    Return(TextSize),
    Break(TextSize),
    Goto(TextSize),
    Label(TextSize),
    Unreachable(TextSize),
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaFlowEdgeKindSummary {
    BlockToStatement,
    StatementFallthrough,
    StatementToBlock,
    StatementToCondition,
    ConditionEnter,
    StatementToBranch,
    ConditionTrue,
    ConditionFalse,
    BranchToClause,
    ClauseToMerge,
    StatementToLoop,
    LoopToBody,
    LoopContinue,
    LoopToMerge,
    MergeToNext,
    StatementToTerminal,
    StatementToLabel,
    LabelToNext,
    TerminalToTarget,
    TerminalToUnreachable,
    GotoToLabel,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowEdgeSummary {
    pub kind: SalsaFlowEdgeKindSummary,
    pub from: SalsaFlowNodeRefSummary,
    pub to: SalsaFlowNodeRefSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowTerminalEdgeSummary {
    pub kind: SalsaFlowTerminalEdgeKindSummary,
    pub syntax_offset: TextSize,
    pub source_block_offset: Option<TextSize>,
    pub target_block_offset: Option<TextSize>,
    pub target_label_offset: Option<TextSize>,
    pub flow_region_root_offset: Option<TextSize>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowQuerySummary {
    pub root_block_offsets: Vec<TextSize>,
    pub branch_links: Vec<SalsaFlowBranchLinkSummary>,
    pub loop_links: Vec<SalsaFlowLoopLinkSummary>,
    pub return_links: Vec<SalsaFlowReturnLinkSummary>,
    pub break_links: Vec<SalsaFlowBreakLinkSummary>,
    pub goto_links: Vec<SalsaFlowGotoLinkSummary>,
    pub terminal_edges: Vec<SalsaFlowTerminalEdgeSummary>,
    pub edges: Vec<SalsaFlowEdgeSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowConditionGraphSummary {
    pub condition_node_offset: u32,
    pub syntax_offset: TextSize,
    pub kind: SalsaFlowConditionKindSummary,
    pub enter_targets: Vec<SalsaFlowNodeRefSummary>,
    pub true_targets: Vec<SalsaFlowNodeRefSummary>,
    pub false_targets: Vec<SalsaFlowNodeRefSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowBranchGraphSummary {
    pub branch_offset: TextSize,
    pub condition_node_offset: Option<u32>,
    pub clause_block_offsets: Vec<TextSize>,
    pub merge_node: Option<SalsaFlowNodeRefSummary>,
    pub next_targets: Vec<SalsaFlowNodeRefSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowLoopGraphSummary {
    pub loop_offset: TextSize,
    pub condition_node_offset: Option<u32>,
    pub body_block_offset: Option<TextSize>,
    pub continue_targets: Vec<SalsaFlowNodeRefSummary>,
    pub merge_node: Option<SalsaFlowNodeRefSummary>,
    pub next_targets: Vec<SalsaFlowNodeRefSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFlowTerminalGraphSummary {
    pub terminal_node: SalsaFlowNodeRefSummary,
    pub target_nodes: Vec<SalsaFlowNodeRefSummary>,
    pub unreachable_node: Option<SalsaFlowNodeRefSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaForRangeIterResolveStateSummary {
    Resolved,
    Partial,
    RecursiveDependency,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaForRangeIterSourceKindSummary {
    Name,
    Member,
    Call,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaForRangeIterSourceSummary {
    pub expr_offset: TextSize,
    pub kind: SalsaForRangeIterSourceKindSummary,
    pub name_type: Option<crate::SalsaProgramPointTypeInfoSummary>,
    pub member_type: Option<crate::SalsaProgramPointMemberTypeInfoSummary>,
    pub call: Option<crate::SalsaCallExplainSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaForRangeIterVarSummary {
    pub name: SmolStr,
    pub decl_id: crate::SalsaDeclId,
    pub slot_index: usize,
    pub type_offsets: Vec<crate::SalsaDocTypeNodeKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaForRangeIterQuerySummary {
    pub loop_offset: TextSize,
    pub iter_expr_offsets: Vec<TextSize>,
    pub state: SalsaForRangeIterResolveStateSummary,
    pub source: Option<SalsaForRangeIterSourceSummary>,
    pub iter_vars: Vec<SalsaForRangeIterVarSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaForRangeIterQueryIndex {
    pub loops: Vec<SalsaForRangeIterQuerySummary>,
}

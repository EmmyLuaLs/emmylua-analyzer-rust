use emmylua_parser::LuaAst;

#[allow(unused)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlowNodeKind {
    None,
    Unreachable,
    Start,
    BranchLabel,
    LoopLabel,
    Assignment,
    TrueCondition,
    FalseCondition,
    Call,
}

#[allow(unused)]
impl FlowNodeKind {
    pub fn is_label(&self) -> bool {
        matches!(self, FlowNodeKind::BranchLabel | FlowNodeKind::LoopLabel)
    }

    pub fn is_condition(&self) -> bool {
        matches!(
            self,
            FlowNodeKind::TrueCondition | FlowNodeKind::FalseCondition
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Copy, Default)]
pub struct FlowId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum FlowAntecedent {
    Node(FlowId),
    Multiple(u32 /* index in multiple_antecedents*/),
}

#[allow(unused)]
#[derive(Debug)]
pub struct FlowNode {
    pub id: FlowId,
    pub kind: FlowNodeKind,
    pub node: Option<LuaAst>,
    pub antecedent: Option<FlowAntecedent>,
}

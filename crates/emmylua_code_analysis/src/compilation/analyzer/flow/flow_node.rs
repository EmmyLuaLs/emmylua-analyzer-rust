#[allow(unused)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlowNodeKind {
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

#[allow(unused)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowAntecedent {
    None,
    Node(usize),
    Multiple(Box<Vec<usize>>),
}

#[allow(unused)]
#[derive(Debug)]
pub struct FlowNode {
    pub id: usize,
    pub kind: FlowNodeKind,
    pub antecedent: FlowAntecedent,
}

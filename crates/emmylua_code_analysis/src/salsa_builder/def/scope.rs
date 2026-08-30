use rowan::TextSize;

use super::SemanticId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScopeKind {
    Chunk,
    Block,
    LocalStat,
    AssignStat,
    ForStat,
    ForRangeStat,
    FuncStat,
    Closure,
    Repeat,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ScopeChild {
    Scope(u32),
    Decl(SemanticId),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Scope {
    pub id: u32,
    pub parent: Option<u32>,
    pub kind: ScopeKind,
    pub start: TextSize,
    pub end: TextSize,
    pub children: Vec<ScopeChild>,
}

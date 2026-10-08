#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AsyncState {
    None,
    Async,
    Sync,
}

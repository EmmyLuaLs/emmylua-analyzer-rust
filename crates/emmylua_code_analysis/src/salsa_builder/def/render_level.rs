#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderLevel {
    Documentation,
    // do not set more than 255
    CustomDetailed(u8),
    Detailed,
    Simple,
    Normal,
    Brief,
    Minimal,
}

#[allow(unused)]
impl RenderLevel {
    pub fn next_level(self) -> RenderLevel {
        match self {
            RenderLevel::Documentation => RenderLevel::Simple,
            RenderLevel::CustomDetailed(_) => RenderLevel::Simple,
            RenderLevel::Detailed => RenderLevel::Simple,
            RenderLevel::Simple => RenderLevel::Normal,
            RenderLevel::Normal => RenderLevel::Brief,
            RenderLevel::Brief => RenderLevel::Minimal,
            RenderLevel::Minimal => RenderLevel::Minimal,
        }
    }

    fn max_items(self) -> usize {
        match self {
            RenderLevel::Documentation => 500,
            RenderLevel::CustomDetailed(n) => n as usize,
            RenderLevel::Detailed => 10,
            RenderLevel::Simple => 8,
            RenderLevel::Normal => 4,
            RenderLevel::Brief => 2,
            RenderLevel::Minimal => 2,
        }
    }

    fn max_union_items(self) -> usize {
        match self {
            RenderLevel::Documentation => 500,
            RenderLevel::CustomDetailed(n) => n as usize,
            RenderLevel::Detailed => 8,
            RenderLevel::Simple => 6,
            RenderLevel::Normal => 4,
            RenderLevel::Brief => 2,
            RenderLevel::Minimal => 2,
        }
    }

    fn max_display_count(self) -> Option<usize> {
        match self {
            RenderLevel::Documentation => Some(500),
            RenderLevel::CustomDetailed(n) => Some(n as usize),
            RenderLevel::Detailed => Some(12),
            _ => None,
        }
    }
}

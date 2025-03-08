use rowan::TextRange;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRange {
    pub start_offset: usize,
    pub length: usize,
}

impl SourceRange {
    pub fn new(start_offset: usize, length: usize) -> SourceRange {
        SourceRange {
            start_offset,
            length,
        }
    }

    pub const EMPTY: SourceRange = SourceRange {
        start_offset: 0,
        length: 0,
    };

    pub fn end_offset(&self) -> usize {
        self.start_offset + self.length
    }

    pub fn contain(&self, offset: usize) -> bool {
        offset >= self.start_offset && offset < self.end_offset()
    }

    pub fn contain_range(&self, range: &SourceRange) -> bool {
        range.start_offset >= self.start_offset && range.end_offset() <= self.end_offset()
    }

    pub fn intersect(&self, range: &SourceRange) -> bool {
        self.start_offset < range.end_offset() && range.start_offset < self.end_offset()
    }

    pub fn merge(&self, range: &SourceRange) -> SourceRange {
        let start = self.start_offset.min(range.start_offset);
        let end = self.end_offset().max(range.end_offset());
        SourceRange {
            start_offset: start,
            length: end - start,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
}

impl std::fmt::Display for SourceRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}, {})", self.start_offset, self.end_offset())
    }
}

impl Into<TextRange> for SourceRange {
    fn into(self) -> TextRange {
        TextRange::new(
            (self.start_offset as u32).into(),
            (self.end_offset() as u32).into(),
        )
    }
}

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

    pub fn from_start_end(start_offset: usize, end_offset: usize) -> Self {
        assert!(start_offset <= end_offset);
        Self::new(start_offset, end_offset - start_offset)
    }

    pub const EMPTY: SourceRange = SourceRange {
        start_offset: 0,
        length: 0,
    };

    pub fn end_offset(&self) -> usize {
        self.start_offset + self.length
    }

    pub fn contains(&self, offset: usize) -> bool {
        offset >= self.start_offset && offset < self.end_offset()
    }

    pub fn contains_inclusive(&self, offset: usize) -> bool {
        offset >= self.start_offset && offset <= self.end_offset()
    }

    pub fn contain_range(&self, range: &SourceRange) -> bool {
        range.start_offset >= self.start_offset && range.end_offset() <= self.end_offset()
    }

    pub fn intersect(&self, range: &SourceRange) -> bool {
        self.start_offset < range.end_offset() && range.start_offset < self.end_offset()
    }

    pub fn moved(&self, offset: usize) -> SourceRange {
        debug_assert!(offset <= self.length);
        SourceRange::new(self.start_offset + offset, self.length - offset)
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

impl From<SourceRange> for TextRange {
    fn from(val: SourceRange) -> Self {
        TextRange::new(
            (val.start_offset as u32).into(),
            (val.end_offset() as u32).into(),
        )
    }
}

impl From<TextRange> for SourceRange {
    fn from(val: TextRange) -> Self {
        SourceRange::new(val.start().into(), val.len().into())
    }
}

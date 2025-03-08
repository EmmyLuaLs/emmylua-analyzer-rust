use emmylua_code_analysis::LuaDocument;
use emmylua_parser::LuaChunk;
use lsp_types::{FoldingRange, FoldingRangeKind};
use rowan::TextRange;

#[derive(Debug)]
pub struct FoldingRangeBuilder<'a> {
    document: &'a LuaDocument<'a>,
    root: LuaChunk,
    folding_ranges: Vec<FoldingRange>,
    region_starts: Vec<TextRange>,
}

impl FoldingRangeBuilder<'_> {
    pub fn new<'a>(document: &'a LuaDocument<'a>, root: LuaChunk) -> FoldingRangeBuilder<'a> {
        FoldingRangeBuilder {
            document,
            root,
            folding_ranges: Vec::new(),
            region_starts: Vec::new(),
        }
    }

    pub fn get_root(&self) -> &LuaChunk {
        &self.root
    }

    pub fn get_document(&self) -> &LuaDocument {
        self.document
    }

    pub fn build(self) -> Vec<FoldingRange> {
        self.folding_ranges
    }

    pub fn push(&mut self, folding_range: FoldingRange) {
        self.folding_ranges.push(folding_range);
    }

    pub fn begin_region(&mut self, range: TextRange) {
        self.region_starts.push(range);
    }

    pub fn finish_region(&mut self, range: TextRange) -> Option<()> {
        if let Some(start) = self.region_starts.pop() {
            let document = self.get_document();
            let region_start_offset = start.start().min(range.start());
            let region_end_offset = start.end().max(range.end());

            let region_start = document.get_line_col(region_start_offset)?;
            let region_end = document.get_line_col(region_end_offset)?;

            let folding_range = FoldingRange {
                start_line: region_start.0 as u32,
                start_character: Some(region_start.1 as u32),
                end_line: region_end.0 as u32,
                end_character: Some(region_end.1 as u32),
                kind: Some(FoldingRangeKind::Region),
                collapsed_text: Some("region".to_string()),
            };

            self.push(folding_range);
        }

        Some(())
    }
}

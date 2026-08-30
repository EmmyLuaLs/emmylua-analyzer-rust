use emmylua_code_analysis::DocumentView;
use emmylua_parser::{LuaAstNode, LuaBlock, LuaChunk, LuaTokenKind};
use lsp_types::{FoldingRange, FoldingRangeKind};
use rowan::TextRange;

use crate::context::ClientId;

#[derive(Debug)]
pub struct FoldingRangeBuilder<'a> {
    document: &'a DocumentView,
    root: LuaChunk,
    folding_ranges: Vec<FoldingRange>,
    region_starts: Vec<TextRange>,
    client_id: ClientId,
}

impl FoldingRangeBuilder<'_> {
    pub fn new<'a>(
        document: &'a DocumentView,
        root: LuaChunk,
        client_id: ClientId,
    ) -> FoldingRangeBuilder<'a> {
        FoldingRangeBuilder {
            document,
            root,
            folding_ranges: Vec::new(),
            region_starts: Vec::new(),
            client_id,
        }
    }

    pub fn get_root(&self) -> &LuaChunk {
        &self.root
    }

    pub fn get_document(&'_ self) -> &'_ DocumentView {
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

    pub fn get_block_collapsed_range(&self, block: LuaBlock) -> Option<lsp_types::Range> {
        let syntax_node = block.syntax();

        let mut prefix_node = syntax_node.prev_sibling_or_token()?;
        while matches!(
            prefix_node.kind().into(),
            LuaTokenKind::TkShortComment | LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine
        ) {
            prefix_node = prefix_node.prev_sibling_or_token()?;
        }

        let mut next_node = match syntax_node.next_sibling_or_token() {
            Some(node) => node,
            None => {
                let parent = syntax_node.parent()?;
                parent.next_sibling_or_token()?
            }
        };
        while matches!(
            next_node.kind().into(),
            LuaTokenKind::TkShortComment | LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine
        ) {
            next_node = next_node.next_sibling_or_token()?;
        }

        let document = self.get_document();
        let (start_line, start_col) = document.get_line_col(prefix_node.text_range().end())?;
        let (end_line, end_col) = document.get_line_col(next_node.text_range().start())?;

        self.get_folding_lsp_range(start_line, end_line, start_col, end_col)
    }

    pub fn get_folding_lsp_range(
        &self,
        start_line: usize,
        end_line: usize,
        start_col: usize,
        end_col: usize,
    ) -> Option<lsp_types::Range> {
        if start_line == end_line {
            return None;
        }

        match self.client_id {
            // Intellij supports range folding.
            ClientId::Intellij => {
                let start_col = start_col + 1;
                let range = lsp_types::Range {
                    start: lsp_types::Position {
                        line: start_line as u32,
                        character: start_col as u32,
                    },
                    end: lsp_types::Position {
                        line: end_line as u32,
                        character: end_col as u32,
                    },
                };

                Some(range)
            }
            _ => {
                // this is impossible
                if end_line == 0 {
                    return None;
                }

                let end_line = end_line - 1;
                if start_line == end_line {
                    return None;
                }

                let range = lsp_types::Range {
                    start: lsp_types::Position {
                        line: start_line as u32,
                        character: 0,
                    },
                    end: lsp_types::Position {
                        line: end_line as u32,
                        character: 0,
                    },
                };
                Some(range)
            }
        }
    }
}

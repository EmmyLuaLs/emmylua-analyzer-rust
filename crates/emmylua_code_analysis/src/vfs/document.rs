use std::path::PathBuf;

use emmylua_parser::LineIndex;
use lsp_types::Uri;
use rowan::{TextRange, TextSize};

use super::{file_path_to_uri, FileId};

#[derive(Debug)]
pub struct LuaDocument<'a> {
    file_id: FileId,
    path: &'a PathBuf,
    text: &'a str,
    line_index: &'a LineIndex,
}

impl<'a> LuaDocument<'a> {
    pub fn new(
        file_id: FileId,
        path: &'a PathBuf,
        text: &'a str,
        line_index: &'a LineIndex,
    ) -> Self {
        LuaDocument {
            file_id,
            path,
            text,
            line_index,
        }
    }

    pub fn get_file_id(&self) -> FileId {
        self.file_id
    }

    pub fn get_file_name(&self) -> Option<String> {
        self.path.file_name()?.to_str().map(|s| s.to_string())
    }

    pub fn get_uri(&self) -> Uri {
        file_path_to_uri(self.path).unwrap()
    }

    pub fn get_file_path(&self) -> &PathBuf {
        self.path
    }

    pub fn get_text(&self) -> &str {
        self.text
    }

    pub fn get_text_slice(&self, range: TextRange) -> &str {
        &self.text[range.start().into()..range.end().into()]
    }

    pub fn get_line_count(&self) -> usize {
        self.line_index.line_count()
    }

    pub fn get_line(&self, offset: TextSize) -> Option<usize> {
        self.line_index.get_line(offset)
    }

    pub fn get_line_col(&self, offset: TextSize) -> Option<(usize, usize)> {
        self.line_index.get_line_col(offset, self.text)
    }

    pub fn get_col(&self, offset: TextSize) -> Option<usize> {
        self.line_index.get_col(offset, self.text)
    }

    pub fn get_offset(&self, line: usize, col: usize) -> Option<TextSize> {
        self.line_index.get_offset(line, col, self.text)
    }

    pub fn get_line_range(&self, line: usize) -> Option<TextRange> {
        let start = self.line_index.get_line_offset(line)?;
        if let Some(end) = self.line_index.get_line_offset(line + 1) {
            Some(TextRange::new(start, end))
        } else {
            let end = TextSize::new(self.text.len() as u32);
            if end > start {
                Some(TextRange::new(start, end))
            } else {
                None
            }
        }
    }

    pub fn to_lsp_range(&self, range: TextRange) -> Option<lsp_types::Range> {
        let start = self.get_line_col(range.start())?;
        let end = self.get_line_col(range.end())?;
        Some(lsp_types::Range {
            start: lsp_types::Position {
                line: start.0 as u32,
                character: start.1 as u32,
            },
            end: lsp_types::Position {
                line: end.0 as u32,
                character: end.1 as u32,
            },
        })
    }

    pub fn to_lsp_location(&self, range: TextRange) -> Option<lsp_types::Location> {
        Some(lsp_types::Location {
            uri: self.get_uri(),
            range: self.to_lsp_range(range)?,
        })
    }

    pub fn to_lsp_position(&self, offset: TextSize) -> Option<lsp_types::Position> {
        let line_col = self.get_line_col(offset)?;
        Some(lsp_types::Position {
            line: line_col.0 as u32,
            character: line_col.1 as u32,
        })
    }

    pub fn to_rowan_range(&self, range: lsp_types::Range) -> Option<TextRange> {
        let start = self.get_offset(range.start.line as usize, range.start.character as usize)?;
        let end = self.get_offset(range.end.line as usize, range.end.character as usize)?;
        Some(TextRange::new(start, end))
    }

    pub fn get_document_lsp_range(&self) -> lsp_types::Range {
        lsp_types::Range {
            start: lsp_types::Position {
                line: 0,
                character: 0,
            },
            end: lsp_types::Position {
                line: self.get_line_count() as u32,
                character: 0,
            },
        }
    }

    pub fn get_range_span(&self, range: lsp_types::Range) -> Option<(usize, usize)> {
        let start = self.get_offset(range.start.line as usize, range.start.character as usize)?;
        let end = self.get_offset(range.end.line as usize, range.end.character as usize)?;
        Some((start.into(), end.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Emmyrc, Vfs, VirtualUrlGenerator};
    use lsp_types::{Position, Range};

    fn create_vfs() -> Vfs {
        let mut vfs = Vfs::new();
        vfs.update_config(Emmyrc::default().into());
        vfs
    }

    #[test]
    fn test_basic() {
        let code = r#"
        local a = 1
        print(a)
        "#;
        let mut vfs = create_vfs();
        let vg = VirtualUrlGenerator::new();
        let uri = vg.new_uri("test.lua");
        let id = vfs.set_file_content(&uri, Some(code.to_string()));
        let document = vfs.get_document(&id).unwrap();

        assert_eq!(document.get_file_id(), id);
        assert_eq!(document.get_file_name(), Some("test.lua".to_string()));
        assert_eq!(document.get_uri(), uri);
        assert_eq!(*document.get_file_path(), vg.new_path("test.lua"));
        assert!(document.get_line_count() > 0, "Document should have lines");
    }

    #[test]
    fn test_text_slice() {
        let code = "Hello, World!";
        let mut vfs = create_vfs();
        let vg = VirtualUrlGenerator::new();
        let uri = vg.new_uri("slice.lua");
        let id = vfs.set_file_content(&uri, Some(code.to_string()));
        let document = vfs.get_document(&id).unwrap();

        // Slice "Hello" from "Hello, World!"
        let range = TextRange::new(TextSize::from(0), TextSize::from(5));
        assert_eq!(document.get_text_slice(range), "Hello");
    }

    #[test]
    fn test_to_lsp_conversions() {
        // Create a document with three lines.
        let code = "line1\nline2\nline3";
        let mut vfs = create_vfs();
        let vg = VirtualUrlGenerator::new();
        let uri = vg.new_uri("conversion.lua");
        let id = vfs.set_file_content(&uri, Some(code.to_string()));
        let document = vfs.get_document(&id).unwrap();

        // Test conversion of offset to lsp position.
        // "line1\n" has 6 bytes so offset 6 should be at the start of "line2".
        let lsp_position = document.to_lsp_position(TextSize::from(6)).unwrap();
        assert_eq!(lsp_position.line, 1);
        assert_eq!(lsp_position.character, 0);

        // Test converting a text range (offset from start of "line2" to its end) to an lsp range.
        // "line2" is 5 characters long, starting at offset 6.
        let range = TextRange::new(TextSize::from(6), TextSize::from(11));
        let lsp_range = document.to_lsp_range(range).unwrap();
        assert_eq!(
            lsp_range.start,
            Position {
                line: 1,
                character: 0
            }
        );
        assert_eq!(
            lsp_range.end,
            Position {
                line: 1,
                character: 5
            }
        );

        // Test converting an lsp range back to a rowan range.
        let rowan_range = document.to_rowan_range(lsp_range).unwrap();
        assert_eq!(rowan_range.start(), TextSize::from(6));
        assert_eq!(rowan_range.end(), TextSize::from(11));
    }

    #[test]
    fn test_range_span() {
        let code = "abcde";
        let mut vfs = create_vfs();
        let vg = VirtualUrlGenerator::new();
        let uri = vg.new_uri("span.lua");
        let id = vfs.set_file_content(&uri, Some(code.to_string()));
        let document = vfs.get_document(&id).unwrap();

        let lsp_range = Range {
            start: Position {
                line: 0,
                character: 1,
            },
            end: Position {
                line: 0,
                character: 4,
            },
        };
        let span = document.get_range_span(lsp_range).unwrap();
        // Expecting span (1, 4) which corresponds to substring "bcd".
        assert_eq!(span, (1, 4));
    }

    #[test]
    fn test_file_name_and_uri() {
        let code = "";
        let mut vfs = create_vfs();
        let vg = VirtualUrlGenerator::new();
        let uri = vg.new_uri("filename_test.lua");
        let id = vfs.set_file_content(&uri, Some(code.to_string()));
        let document = vfs.get_document(&id).unwrap();

        assert_eq!(
            document.get_file_name(),
            Some("filename_test.lua".to_string())
        );
        assert_eq!(document.get_uri(), uri);
    }

    #[test]
    fn test_document_lsp_range() {
        let code = "one\ntwo\nthree";
        let mut vfs = create_vfs();
        let vg = VirtualUrlGenerator::new();
        let uri = vg.new_uri("doc_range.lua");
        let id = vfs.set_file_content(&uri, Some(code.to_string()));
        let document = vfs.get_document(&id).unwrap();

        let doc_range = document.get_document_lsp_range();
        // The document range should start at line 0 and end at line count.
        assert_eq!(
            doc_range.start,
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(doc_range.end.line, document.get_line_count() as u32);
        assert_eq!(doc_range.end.character, 0);
    }
}

use rowan::TextSize;

#[derive(Debug, Clone)]
pub struct LineIndex {
    line_offsets: Vec<u32>,
    line_only_ascii_vec: Vec<bool>,
}

impl LineIndex {
    pub fn parse(text: &str) -> LineIndex {
        let mut line_offsets = Vec::new();
        let mut line_only_ascii_vec = Vec::new();
        line_offsets.push(0);

        let mut is_line_only_ascii = true;
        for (index, byte) in text.as_bytes().iter().copied().enumerate() {
            if byte == b'\n' {
                line_offsets.push((index + 1) as u32);
                line_only_ascii_vec.push(is_line_only_ascii);
                is_line_only_ascii = true;
            } else if byte >= 0x80 {
                is_line_only_ascii = false;
            }
        }

        line_only_ascii_vec.push(is_line_only_ascii);

        assert_eq!(line_offsets.len(), line_only_ascii_vec.len());
        LineIndex {
            line_offsets,
            line_only_ascii_vec,
        }
    }

    pub fn get_line_offset(&self, line: usize) -> Option<TextSize> {
        let line_index = line;
        if line_index < self.line_offsets.len() {
            Some(TextSize::from(self.line_offsets[line_index]))
        } else {
            None
        }
    }

    // get line base 0
    pub fn get_line(&self, offset: TextSize) -> Option<usize> {
        let offset = u32::from(offset);
        let line = self
            .line_offsets
            .partition_point(|&line_offset| line_offset <= offset);
        if line == 0 { None } else { Some(line - 1) }
    }

    pub fn get_line_with_start_offset(&self, offset: TextSize) -> Option<(usize, TextSize)> {
        let line = self.get_line(offset)?;
        let start_offset = TextSize::from(self.line_offsets[line]);
        Some((line, start_offset))
    }

    #[inline]
    fn is_line_only_ascii_index(&self, line: usize) -> bool {
        self.line_only_ascii_vec.get(line).copied().unwrap_or(false)
    }

    pub fn is_line_only_ascii(&self, line: TextSize) -> bool {
        self.is_line_only_ascii_index(usize::from(line))
    }

    pub fn line_count(&self) -> usize {
        self.line_offsets.len()
    }

    // get col base 0
    pub fn get_col(&self, offset: TextSize, source_text: &str) -> Option<usize> {
        let (line, start_offset) = self.get_line_with_start_offset(offset)?;
        if self.is_line_only_ascii_index(line) {
            Some(usize::from(offset - start_offset))
        } else {
            let text = &source_text[usize::from(start_offset)..usize::from(offset)];
            Some(text.chars().count())
        }
    }

    // get line and col base 0
    pub fn get_line_col(&self, offset: TextSize, source_text: &str) -> Option<(usize, usize)> {
        let (line, start_offset) = self.get_line_with_start_offset(offset)?;
        if self.is_line_only_ascii_index(line) {
            Some((line, usize::from(offset - start_offset)))
        } else {
            let text = &source_text[usize::from(start_offset)..usize::from(offset)];
            Some((line, text.chars().count()))
        }
    }

    // get offset by line and col
    pub fn get_offset(&self, line: usize, col: usize, source_text: &str) -> Option<TextSize> {
        let start_offset = self.get_line_offset(line)?;
        if col == 0 {
            return Some(start_offset);
        }

        if self.is_line_only_ascii_index(line) {
            let col = col.min(source_text.len());
            Some(start_offset + TextSize::from(col as u32))
        } else {
            let mut offset = 0;
            let mut col = col;
            for c in source_text[usize::from(start_offset)..].chars() {
                if col == 0 {
                    break;
                }

                offset += c.len_utf8();
                col -= 1;
            }
            Some(start_offset + TextSize::from(offset as u32))
        }
    }

    pub fn get_col_offset_at_line(
        &self,
        line: usize,
        col: usize,
        source_text: &str,
    ) -> Option<TextSize> {
        let start_offset = self.get_line_offset(line)?;
        if col == 0 {
            return Some(0.into());
        }

        if self.is_line_only_ascii_index(line) {
            let col = col.min(source_text.len());
            Some(TextSize::from(col as u32))
        } else {
            let mut offset = 0;
            let mut col = col;
            for c in source_text[usize::from(start_offset)..].chars() {
                if col == 0 {
                    break;
                }

                offset += c.len_utf8();
                col -= 1;
            }
            Some(TextSize::from(offset as u32))
        }
    }
}

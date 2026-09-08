use emmylua_code_analysis::LuaDocument;
use lsp_types::{Position, Range, TextEdit};

/// Represents the type of line difference
#[derive(Debug, Clone)]
enum LineDiff {
    /// Line remains unchanged
    Unchanged(usize),
    /// Line is deleted
    Deleted(usize),
    /// Line is added
    Added(usize),
}

/// Ultra-fast diff algorithm - optimized for formatting scenarios
/// For most formatting cases, changes are minimal, so a fast heuristic algorithm is used
fn compute_line_diff_ultra_fast(source_lines: &[&str], formatted_lines: &[&str]) -> Vec<LineDiff> {
    let n = source_lines.len();
    let m = formatted_lines.len();

    // Early exit: completely identical
    if n == m && source_lines == formatted_lines {
        return (0..n).map(LineDiff::Unchanged).collect();
    }

    // Early exit: one is empty
    if n == 0 {
        return (0..m).map(LineDiff::Added).collect();
    }
    if m == 0 {
        return (0..n).map(LineDiff::Deleted).collect();
    }

    // For small files, use a simple but fast algorithm
    if n <= 100 && m <= 100 {
        return simple_line_diff_optimized(source_lines, formatted_lines);
    }

    // For large files, use a heuristic algorithm
    heuristic_line_diff(source_lines, formatted_lines)
}

/// Simple algorithm optimized for small files
fn simple_line_diff_optimized(source_lines: &[&str], formatted_lines: &[&str]) -> Vec<LineDiff> {
    let mut diffs = Vec::new();
    let mut i = 0;
    let mut j = 0;

    while i < source_lines.len() && j < formatted_lines.len() {
        if source_lines[i] == formatted_lines[j] {
            diffs.push(LineDiff::Unchanged(i));
            i += 1;
            j += 1;
        } else {
            // Use a sliding window to find the nearest match
            let window_size = 5.min(source_lines.len() - i).min(formatted_lines.len() - j);
            let mut found_match = false;

            // Check for deletion
            for di in 1..=window_size {
                if i + di < source_lines.len() && source_lines[i + di] == formatted_lines[j] {
                    // Delete lines i..i+di
                    for k in 0..di {
                        diffs.push(LineDiff::Deleted(i + k));
                    }
                    i += di;
                    found_match = true;
                    break;
                }
            }

            if !found_match {
                // Check for addition
                for dj in 1..=window_size {
                    if j + dj < formatted_lines.len() && source_lines[i] == formatted_lines[j + dj]
                    {
                        // Add lines j..j+dj
                        for k in 0..dj {
                            diffs.push(LineDiff::Added(j + k));
                        }
                        j += dj;
                        found_match = true;
                        break;
                    }
                }
            }

            if !found_match {
                // No match found, treat as delete + add
                diffs.push(LineDiff::Deleted(i));
                diffs.push(LineDiff::Added(j));
                i += 1;
                j += 1;
            }
        }
    }

    // Handle remaining lines
    while i < source_lines.len() {
        diffs.push(LineDiff::Deleted(i));
        i += 1;
    }
    while j < formatted_lines.len() {
        diffs.push(LineDiff::Added(j));
        j += 1;
    }

    diffs
}

/// Heuristic algorithm for large files
fn heuristic_line_diff(source_lines: &[&str], formatted_lines: &[&str]) -> Vec<LineDiff> {
    // For large files, process in chunks and then merge results
    let chunk_size = 200;
    let mut all_diffs = Vec::new();
    let mut source_offset = 0;
    let mut formatted_offset = 0;

    while source_offset < source_lines.len() || formatted_offset < formatted_lines.len() {
        let source_end = (source_offset + chunk_size).min(source_lines.len());
        let formatted_end = (formatted_offset + chunk_size).min(formatted_lines.len());

        let source_chunk = &source_lines[source_offset..source_end];
        let formatted_chunk = &formatted_lines[formatted_offset..formatted_end];

        let chunk_diffs = simple_line_diff_optimized(source_chunk, formatted_chunk);

        // Adjust index offsets
        for diff in chunk_diffs {
            match diff {
                LineDiff::Unchanged(idx) => {
                    all_diffs.push(LineDiff::Unchanged(idx + source_offset))
                }
                LineDiff::Deleted(idx) => all_diffs.push(LineDiff::Deleted(idx + source_offset)),
                LineDiff::Added(idx) => all_diffs.push(LineDiff::Added(idx + formatted_offset)),
            }
        }

        source_offset = source_end;
        formatted_offset = formatted_end;
    }

    all_diffs
}

/// Count the number of changes (excluding unchanged lines)
fn count_changes(diffs: &[LineDiff]) -> usize {
    diffs
        .iter()
        .filter(|diff| !matches!(diff, LineDiff::Unchanged(_)))
        .count()
}

/// Generate a list of TextEdit from line diffs (optimized version)
fn generate_text_edits(
    diffs: &[LineDiff],
    _source_lines: &[&str],
    formatted_lines: &[&str],
    document: &LuaDocument,
) -> Vec<TextEdit> {
    // Pre-allocate capacity to reduce Vec reallocations
    let mut edits = Vec::with_capacity(diffs.len().min(100)); // Limit max pre-allocation to avoid memory waste

    // Batch consecutive edit operations to reduce the number of TextEdits
    let mut i = 0;
    while i < diffs.len() {
        match &diffs[i] {
            LineDiff::Unchanged(_) => {
                i += 1;
            }
            LineDiff::Deleted(_) => {
                // Find consecutive delete operations
                let start_idx = i;
                while i < diffs.len() {
                    if let LineDiff::Deleted(_) = diffs[i] {
                        i += 1;
                    } else {
                        break;
                    }
                }

                // Batch delete: calculate start and end positions
                if let LineDiff::Deleted(first_line) = diffs[start_idx]
                    && let LineDiff::Deleted(last_line) = diffs[i - 1]
                    // Get the range from the start of the first line to the end of the last line
                    && let Some(start_range) = document.get_line_range(first_line)
                    && let Some(end_range) = document.get_line_range(last_line)
                {
                    let combined_range =
                        rowan::TextRange::new(start_range.start(), end_range.end());
                    if let Some(lsp_range) = document.to_lsp_range(combined_range) {
                        edits.push(TextEdit {
                            range: lsp_range,
                            new_text: String::new(),
                        });
                    }
                }
            }
            LineDiff::Added(_) => {
                // Find consecutive add operations
                let mut new_text = String::new();
                let insert_line = match diffs.get(i.saturating_sub(1)) {
                    Some(LineDiff::Unchanged(line)) => line + 1,
                    Some(LineDiff::Deleted(line)) => *line,
                    _ => 0,
                };

                while i < diffs.len() {
                    if let LineDiff::Added(formatted_idx) = diffs[i] {
                        new_text.push_str(formatted_lines[formatted_idx]);
                        new_text.push('\n');
                        i += 1;
                    } else {
                        break;
                    }
                }

                // Batch insert
                let insert_position = Position {
                    line: insert_line as u32,
                    character: 0,
                };
                edits.push(TextEdit {
                    range: Range {
                        start: insert_position,
                        end: insert_position,
                    },
                    new_text,
                });
            }
        }
    }

    edits
}

pub fn format_diff(
    source_text: &str,
    formatted_text: &str,
    document: &LuaDocument,
    replace_all_limit: usize,
) -> Vec<TextEdit> {
    // Early exit: if texts are identical, return empty edits
    if source_text == formatted_text {
        return Vec::new();
    }

    // Split text by lines
    let source_lines: Vec<&str> = source_text.lines().collect();
    let formatted_lines: Vec<&str> = formatted_text.lines().collect();

    // Early exit: if line count difference is too large, do a global replace
    let line_diff =
        (source_lines.len() as i32 - formatted_lines.len() as i32).unsigned_abs() as usize;
    if line_diff >= replace_all_limit {
        let document_range = document.get_document_lsp_range();
        return vec![TextEdit {
            range: document_range,
            new_text: formatted_text.to_string(),
        }];
    }

    // Use ultra-fast algorithm to compute line-level diffs
    let diffs = compute_line_diff_ultra_fast(&source_lines, &formatted_lines);

    // Count the number of changes
    let change_count = count_changes(&diffs);

    // If the number of changes exceeds the limit, do a global replace
    if change_count >= replace_all_limit {
        let document_range = document.get_document_lsp_range();
        return vec![TextEdit {
            range: document_range,
            new_text: formatted_text.to_string(),
        }];
    }

    // Otherwise, generate detailed edit operations
    generate_text_edits(&diffs, &source_lines, &formatted_lines, document)
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    /// Generate test code text
    fn generate_test_code(lines: usize, changes_percent: f32) -> (String, String) {
        let mut source_lines = Vec::new();
        let mut formatted_lines = Vec::new();

        for i in 0..lines {
            let line = format!("function test_function_{}()", i);
            source_lines.push(line.clone());

            // Decide whether to modify this line based on the change percentage
            if (i as f32 / lines as f32) < changes_percent {
                formatted_lines.push(format!("function test_function_{}()\n    -- formatted", i));
            } else {
                formatted_lines.push(line);
            }
        }

        (source_lines.join("\n"), formatted_lines.join("\n"))
    }

    #[test]
    fn benchmark_small_file() {
        let (source, formatted) = generate_test_code(100, 0.1); // 100 lines, 10% changes

        let start = Instant::now();
        let source_lines: Vec<&str> = source.lines().collect();
        let formatted_lines: Vec<&str> = formatted.lines().collect();
        let _diffs = compute_line_diff_ultra_fast(&source_lines, &formatted_lines);
        let duration = start.elapsed();

        println!("Small file (100 lines, 10% changes): {:?}", duration);
        assert!(duration.as_millis() < 10); // Should complete within 10ms
    }

    #[test]
    fn benchmark_medium_file() {
        let (source, formatted) = generate_test_code(1000, 0.05); // 1000 lines, 5% changes

        let start = Instant::now();
        let source_lines: Vec<&str> = source.lines().collect();
        let formatted_lines: Vec<&str> = formatted.lines().collect();
        let _diffs = compute_line_diff_ultra_fast(&source_lines, &formatted_lines);
        let duration = start.elapsed();

        println!("Medium file (1000 lines, 5% changes): {:?}", duration);
        assert!(duration.as_millis() < 50); // Should complete within 50ms
    }

    #[test]
    fn benchmark_large_file() {
        let (source, formatted) = generate_test_code(5000, 0.02); // 5000 lines, 2% changes

        let start = Instant::now();
        let source_lines: Vec<&str> = source.lines().collect();
        let formatted_lines: Vec<&str> = formatted.lines().collect();
        let _diffs = compute_line_diff_ultra_fast(&source_lines, &formatted_lines);
        let duration = start.elapsed();

        println!("Large file (5000 lines, 2% changes): {:?}", duration);
        assert!(duration.as_millis() < 20); // Should complete within 200ms
    }

    #[test]
    fn benchmark_identical_files() {
        let line = "function test()\n    return 42\nend";
        let source = std::iter::repeat_n(line, 1000)
            .collect::<Vec<_>>()
            .join("\n");
        let formatted = source.clone();

        let start = Instant::now();
        let source_lines: Vec<&str> = source.lines().collect();
        let formatted_lines: Vec<&str> = formatted.lines().collect();
        let _diffs = compute_line_diff_ultra_fast(&source_lines, &formatted_lines);
        let duration = start.elapsed();

        println!("Identical files (1000 lines): {:?}", duration);
        assert!(duration.as_millis() < 10); // Should complete within 100 microseconds
    }

    #[test]
    fn test_correctness() {
        let source = "line1\nline2\nline3\nline4";
        let formatted = "line1\nline2_modified\nline3\nline5";

        let source_lines: Vec<&str> = source.lines().collect();
        let formatted_lines: Vec<&str> = formatted.lines().collect();
        let diffs = compute_line_diff_ultra_fast(&source_lines, &formatted_lines);

        // Verify correctness of the result
        let change_count = diffs
            .iter()
            .filter(|d| !matches!(d, LineDiff::Unchanged(_)))
            .count();
        assert!(change_count >= 2); // At least 2 changes (line2_modified, line5 replaces line4)
    }
}

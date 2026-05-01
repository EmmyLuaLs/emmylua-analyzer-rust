use crate::analysis::analyze_lua_doc_file;
use crate::diagnostics::Diagnostics;
use crate::fs::{collect_std_lua_files, path_label};
use crate::model::AnalyzedStdFile;
use crate::target::compute_replace_targets;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct StdI18nProject {
    pub files: Vec<AnalyzedStdFile>,
}

pub fn analyze_std_i18n_project(
    std_dir: &Path,
    include_empty: bool,
    diagnostics: &mut Diagnostics,
) -> Result<StdI18nProject, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    for (rel_path, full_path) in collect_std_lua_files(std_dir)? {
        let content = std::fs::read_to_string(&full_path)?;
        let file_label = path_label(&rel_path);
        let analyzed = analyze_lua_doc_file(&file_label, &content, include_empty, diagnostics);
        let targets = compute_replace_targets(
            &file_label,
            &content,
            &analyzed.comments,
            &analyzed.entries,
            diagnostics,
        );

        out.push(AnalyzedStdFile {
            path: rel_path,
            line_starts: build_line_start_offsets(&content),
            content,
            entries: analyzed.entries,
            targets,
        });
    }

    Ok(StdI18nProject { files: out })
}

pub fn build_line_start_offsets(s: &str) -> Vec<usize> {
    let mut out = Vec::new();
    out.push(0);
    for (i, b) in s.as_bytes().iter().enumerate() {
        if *b == b'\n' {
            out.push(i + 1);
        }
    }
    out
}

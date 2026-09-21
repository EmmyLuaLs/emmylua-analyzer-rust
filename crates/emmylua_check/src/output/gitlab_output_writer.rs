use std::{fs::File, io::Write, path::PathBuf};

use emmylua_code_analysis::{DbIndex, FileId};
use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::cmd_args::OutputDestination;

use super::OutputWriter;

#[derive(Debug, Serialize)]
struct GitlabCodeQualityFinding {
    description: String,
    check_name: String,
    fingerprint: String,
    severity: &'static str,
    location: GitlabCodeQualityLocation,
}

#[derive(Debug, Serialize)]
struct GitlabCodeQualityLocation {
    path: String,
    lines: GitlabCodeQualityLines,
}

#[derive(Debug, Serialize)]
struct GitlabCodeQualityLines {
    begin: u32,
}

pub struct GitlabOutputWriter {
    output: Option<File>,
    project_root: PathBuf,
    findings: Vec<GitlabCodeQualityFinding>,
}

impl GitlabOutputWriter {
    pub fn new(output: OutputDestination, workspace: PathBuf) -> Self {
        let output = match output {
            OutputDestination::Stdout => None,
            OutputDestination::File(path) => {
                if let Some(parent) = path.parent()
                    && !parent.exists()
                {
                    std::fs::create_dir_all(parent).unwrap_or_else(|error| {
                        panic!(
                            "failed to create GitLab Code Quality report directory {}: {error}",
                            parent.display()
                        )
                    });
                }
                Some(File::create(&path).unwrap_or_else(|error| {
                    panic!(
                        "failed to create GitLab Code Quality report {}: {error}",
                        path.display()
                    )
                }))
            }
        };

        // GitLab requires paths relative to the repository, not the analyzed
        // workspace (which may be a subdirectory such as `src`).
        let project_root = std::env::var_os("CI_PROJECT_DIR")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or(workspace);
        let project_root = normalize_path(&project_root);

        Self {
            output,
            project_root,
            findings: Vec::new(),
        }
    }
}

impl OutputWriter for GitlabOutputWriter {
    fn write(&mut self, db: &DbIndex, file_id: FileId, diagnostics: Vec<Diagnostic>) {
        let Some(file_path) = db.get_vfs().get_file_path(&file_id) else {
            return;
        };
        let Some(path) = repository_relative_path(&self.project_root, file_path) else {
            eprintln!(
                "emmylua_check: skipped GitLab Code Quality findings for {} because it is outside the repository root {}",
                file_path.display(),
                self.project_root.display()
            );
            return;
        };

        for diagnostic in diagnostics {
            self.findings.push(convert_diagnostic(&path, &diagnostic));
        }
    }

    fn finish(&mut self) {
        let report = serde_json::to_string_pretty(&self.findings)
            .expect("failed to serialize GitLab Code Quality report");

        if let Some(output) = self.output.as_mut() {
            output
                .write_all(report.as_bytes())
                .expect("failed to write GitLab Code Quality report");
            output
                .write_all(b"\n")
                .expect("failed to finish GitLab Code Quality report");
        } else {
            println!("{report}");
        }
    }
}

fn convert_diagnostic(path: &str, diagnostic: &Diagnostic) -> GitlabCodeQualityFinding {
    let check_name = diagnostic
        .code
        .as_ref()
        .map(|code| match code {
            NumberOrString::Number(number) => number.to_string(),
            NumberOrString::String(name) => name.clone(),
        })
        .unwrap_or_else(|| "emmylua_check".to_string());
    let begin = diagnostic.range.start.line + 1;
    let fingerprint = fingerprint(&check_name, path, begin, &diagnostic.message);

    GitlabCodeQualityFinding {
        description: diagnostic.message.clone(),
        check_name,
        fingerprint,
        severity: gitlab_severity(diagnostic.severity),
        location: GitlabCodeQualityLocation {
            path: path.to_string(),
            lines: GitlabCodeQualityLines { begin },
        },
    }
}

fn gitlab_severity(severity: Option<DiagnosticSeverity>) -> &'static str {
    // GitLab has no direct equivalents for all LSP severities. Keep errors and
    // warnings distinct while grouping non-actionable information and hints.
    match severity {
        Some(DiagnosticSeverity::ERROR) => "major",
        Some(DiagnosticSeverity::WARNING) => "minor",
        _ => "info",
    }
}

fn fingerprint(check_name: &str, path: &str, begin: u32, description: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(check_name.as_bytes());
    hasher.update(b"\0");
    hasher.update(path.as_bytes());
    hasher.update(b"\0");
    hasher.update(begin.to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(description.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn repository_relative_path(
    project_root: &std::path::Path,
    file_path: &std::path::Path,
) -> Option<String> {
    let project_root = normalize_path(project_root);
    let file_path = normalize_path(file_path);
    let relative = file_path.strip_prefix(&project_root).ok()?;
    let normalized = relative.to_string_lossy().replace('\\', "/");
    let normalized = normalized
        .strip_prefix("./")
        .unwrap_or(&normalized)
        .trim_start_matches('/')
        .to_string();
    (!normalized.is_empty()).then_some(normalized)
}

fn normalize_path(path: &std::path::Path) -> PathBuf {
    let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    strip_extended_prefix(&normalized)
}

fn strip_extended_prefix(path: &std::path::Path) -> PathBuf {
    let path = path.to_string_lossy();
    let stripped = path.strip_prefix(r"\\?\").unwrap_or(&path);
    PathBuf::from(stripped.to_string())
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::{cmd_args::OutputDestination, output::OutputWriter};

    use super::{
        GitlabOutputWriter, convert_diagnostic, fingerprint, gitlab_severity,
        repository_relative_path,
    };

    fn diagnostic(
        severity: DiagnosticSeverity,
        code: &str,
        line: u32,
        message: &str,
    ) -> Diagnostic {
        Diagnostic {
            range: Range::new(Position::new(line, 4), Position::new(line, 12)),
            severity: Some(severity),
            code: Some(NumberOrString::String(code.to_string())),
            source: Some("emmylua_check".to_string()),
            message: message.to_string(),
            ..Default::default()
        }
    }

    #[gtest]
    fn diagnostic_is_serialized_with_all_required_gitlab_code_quality_fields() {
        let diagnostic = diagnostic(
            DiagnosticSeverity::ERROR,
            "undefined-global",
            9,
            "Undefined global `player`.",
        );

        let finding = convert_diagnostic("src/player.lua", &diagnostic);
        let actual = serde_json::to_value(finding).unwrap();
        let expected = json!({
            "description": "Undefined global `player`.",
            "check_name": "undefined-global",
            "fingerprint": "41f77fdf3e86cb1db415c90a64922492236149e9468257d06645d145412a43ce",
            "severity": "major",
            "location": {
                "path": "src/player.lua",
                "lines": {
                    "begin": 10
                }
            }
        });

        assert_that!(actual, eq(&expected));
    }

    #[gtest]
    fn every_lsp_severity_maps_to_a_value_accepted_by_gitlab() {
        expect_that!(
            gitlab_severity(Some(DiagnosticSeverity::ERROR)),
            eq("major")
        );
        expect_that!(
            gitlab_severity(Some(DiagnosticSeverity::WARNING)),
            eq("minor")
        );
        expect_that!(
            gitlab_severity(Some(DiagnosticSeverity::INFORMATION)),
            eq("info")
        );
        expect_that!(gitlab_severity(Some(DiagnosticSeverity::HINT)), eq("info"));
        expect_that!(gitlab_severity(None), eq("info"));
    }

    #[gtest]
    fn same_finding_keeps_its_fingerprint_but_path_or_line_changes_it() {
        let base = fingerprint("unused", "src/main.lua", 3, "Unused local `name`.");

        expect_that!(
            base.as_str(),
            eq("7da7e56fb955d3a4b84d6c5fdeb913242a0a8ec057c519dc224e4ba39c05df10")
        );
        expect_that!(
            fingerprint("unused", "src/main.lua", 3, "Unused local `name`."),
            eq(&base)
        );
        expect_that!(
            fingerprint("unused", "src/other.lua", 3, "Unused local `name`."),
            ne(&base)
        );
        expect_that!(
            fingerprint("unused", "src/main.lua", 4, "Unused local `name`."),
            ne(&base)
        );
    }

    #[gtest]
    fn file_under_the_repository_root_is_reported_with_a_relative_path() {
        let path = repository_relative_path(
            std::path::Path::new("/build/project"),
            std::path::Path::new("/build/project/src/main.lua"),
        )
        .unwrap();

        assert_that!(path, eq("src/main.lua"));
    }

    #[gtest]
    fn file_outside_the_repository_root_is_rejected() {
        let path = repository_relative_path(
            std::path::Path::new("/build/project"),
            std::path::Path::new("/build/shared/main.lua"),
        );

        assert_that!(path, none());
    }

    #[gtest]
    fn analysis_without_findings_writes_an_empty_json_array() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "emmylua-check-gitlab-{}-{unique}.json",
            std::process::id()
        ));
        let mut writer = GitlabOutputWriter::new(
            OutputDestination::File(output_path.clone()),
            std::env::temp_dir(),
        );

        writer.finish();
        let report = std::fs::read_to_string(&output_path).unwrap();
        std::fs::remove_file(output_path).unwrap();

        assert_that!(report.as_str(), eq("[]\n"));
    }
}

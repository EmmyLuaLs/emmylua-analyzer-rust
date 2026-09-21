use std::{fs::File, io::Write, path::PathBuf};

use emmylua_code_analysis::{DbIndex, FileId};
use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{cmd_args::OutputDestination, init::normalize_local_path};

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
    pub fn new(output: OutputDestination) -> Self {
        let output = match output {
            OutputDestination::Stdout => None,
            OutputDestination::File(path) => {
                if let Some(parent) = path.parent()
                    && !parent.exists()
                {
                    std::fs::create_dir_all(parent)
                        .expect("failed to create GitLab Code Quality report directory");
                }
                Some(File::create(path).expect("failed to create GitLab Code Quality report"))
            }
        };

        let project_root = normalize_local_path(
            std::env::current_dir().expect("failed to resolve current working directory"),
        );

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
    let fingerprint = fingerprint(&check_name, path, diagnostic);

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

fn fingerprint(check_name: &str, path: &str, diagnostic: &Diagnostic) -> String {
    let mut hasher = Sha256::new();
    hasher.update(check_name.as_bytes());
    hasher.update(b"\0");
    hasher.update(path.as_bytes());
    hasher.update(b"\0");
    hasher.update(diagnostic.range.start.line.to_be_bytes());
    hasher.update(diagnostic.range.start.character.to_be_bytes());
    hasher.update(diagnostic.range.end.line.to_be_bytes());
    hasher.update(diagnostic.range.end.character.to_be_bytes());
    hasher.update(b"\0");
    hasher.update(diagnostic.message.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn repository_relative_path(
    project_root: &std::path::Path,
    file_path: &std::path::Path,
) -> Option<String> {
    let file_path = normalize_local_path(file_path.to_path_buf());
    file_path
        .strip_prefix(project_root)
        .ok()
        .map(|path| {
            path.components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        })
        .filter(|path| !path.is_empty())
}

#[cfg(test)]
mod tests {
    use emmylua_code_analysis::{DbIndex, file_path_to_uri};
    use googletest::prelude::*;
    use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};
    use serde_json::json;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::{cmd_args::OutputDestination, init::normalize_local_path, output::OutputWriter};

    use super::{GitlabOutputWriter, fingerprint, gitlab_severity, repository_relative_path};

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

    fn report_path(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "emmylua-check-gitlab-{label}-{}-{unique}.json",
            std::process::id()
        ))
    }

    #[gtest]
    fn diagnostic_inside_repository_is_written_as_a_complete_gitlab_code_quality_report() {
        let diagnostic = diagnostic(
            DiagnosticSeverity::ERROR,
            "undefined-global",
            9,
            "Undefined global `player`.",
        );
        let repository = normalize_local_path(std::env::current_dir().unwrap());
        let file_path = repository.join("src/player.lua");
        let mut db = DbIndex::new();
        let file_id = db
            .get_vfs_mut()
            .file_id(&file_path_to_uri(&file_path).unwrap());
        let output_path = report_path("finding");
        let mut writer = GitlabOutputWriter::new(OutputDestination::File(output_path.clone()));

        writer.write(&db, file_id, vec![diagnostic]);
        writer.finish();
        let actual = serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(&output_path).unwrap(),
        )
        .unwrap();
        std::fs::remove_file(output_path).unwrap();
        let expected = json!([{
            "description": "Undefined global `player`.",
            "check_name": "undefined-global",
            "fingerprint": "4a5dc7adb2d434995ee8f525c4f55608702935a2309c12b397e1c1f84d15326c",
            "severity": "major",
            "location": {
                "path": "src/player.lua",
                "lines": {
                    "begin": 10
                }
            }
        }]);

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
    fn fingerprint_is_repeatable_and_changes_when_rule_path_range_or_description_changes() {
        let diagnostic = diagnostic(
            DiagnosticSeverity::WARNING,
            "unused",
            2,
            "Unused local `name`.",
        );
        let base = fingerprint("unused", "src/main.lua", &diagnostic);
        let mut changed_range = diagnostic.clone();
        changed_range.range.start.character += 1;
        let mut changed_description = diagnostic.clone();
        changed_description.message.push_str(" Please remove it.");

        expect_that!(
            fingerprint("unused", "src/main.lua", &diagnostic),
            eq(&base)
        );
        expect_that!(
            fingerprint("unused-local", "src/main.lua", &diagnostic),
            ne(&base)
        );
        expect_that!(
            fingerprint("unused", "src/other.lua", &diagnostic),
            ne(&base)
        );
        expect_that!(
            fingerprint("unused", "src/main.lua", &changed_range),
            ne(&base)
        );
        expect_that!(
            fingerprint("unused", "src/main.lua", &changed_description),
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
    fn writer_without_findings_writes_an_empty_json_array() {
        let output_path = report_path("empty");
        let mut writer = GitlabOutputWriter::new(OutputDestination::File(output_path.clone()));

        writer.finish();
        let report = std::fs::read_to_string(&output_path).unwrap();
        std::fs::remove_file(output_path).unwrap();

        assert_that!(report.as_str(), eq("[]\n"));
    }
}

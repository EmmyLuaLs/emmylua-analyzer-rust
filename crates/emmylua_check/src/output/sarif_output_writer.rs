use std::{fs::File, io::Write};

use emmylua_code_analysis::{DbIndex, FileId};
use lsp_types::Diagnostic;
use serde_json::{Value, json};

use crate::cmd_args::OutputDestination;

use super::OutputWriter;

#[derive(Debug)]
pub struct SarifOutputWriter {
    output: Option<File>,
    sarif_runs: Vec<Value>,
}

impl SarifOutputWriter {
    pub fn new(output: OutputDestination) -> Self {
        let output = match output {
            OutputDestination::Stdout => None,
            OutputDestination::File(path) => {
                if let Some(parent) = path.parent() {
                    if !parent.exists() {
                        std::fs::create_dir_all(parent).unwrap();
                    }
                }

                Some(std::fs::File::create(path).unwrap())
            }
        };
        SarifOutputWriter {
            output,
            sarif_runs: Vec::new(),
        }
    }

    fn diagnostic_severity_to_sarif_level(severity: Option<lsp_types::DiagnosticSeverity>) -> &'static str {
        match severity {
            Some(lsp_types::DiagnosticSeverity::ERROR) => "error",
            Some(lsp_types::DiagnosticSeverity::WARNING) => "warning",
            Some(lsp_types::DiagnosticSeverity::INFORMATION) => "note",
            Some(lsp_types::DiagnosticSeverity::HINT) => "note",
            _ => "warning",
        }
    }

    fn create_sarif_result(
        db: &DbIndex,
        file_id: FileId,
        diagnostic: &Diagnostic,
    ) -> Option<Value> {
        let file_path = db.get_vfs().get_file_path(&file_id)?;
        let file_path_str = file_path.to_str()?;

        // Convert diagnostic code to rule ID
        let rule_id = match &diagnostic.code {
            Some(lsp_types::NumberOrString::String(code)) => code.clone(),
            Some(lsp_types::NumberOrString::Number(code)) => code.to_string(),
            None => "unknown".to_string(),
        };

        let level = Self::diagnostic_severity_to_sarif_level(diagnostic.severity);

        Some(json!({
            "ruleId": rule_id,
            "level": level,
            "message": {
                "text": diagnostic.message
            },
            "locations": [{
                "physicalLocation": {
                    "artifactLocation": {
                        "uri": format!("file://{}", file_path_str)
                    },
                    "region": {
                        "startLine": diagnostic.range.start.line + 1,
                        "startColumn": diagnostic.range.start.character + 1,
                        "endLine": diagnostic.range.end.line + 1,
                        "endColumn": diagnostic.range.end.character + 1
                    }
                }
            }]
        }))
    }
}

impl OutputWriter for SarifOutputWriter {
    fn write(&mut self, db: &DbIndex, file_id: FileId, diagnostics: Vec<Diagnostic>) {
        for diagnostic in diagnostics {
            if let Some(result) = Self::create_sarif_result(db, file_id, &diagnostic) {
                self.sarif_runs.push(result);
            }
        }
    }

    fn finish(&mut self) {
        let sarif_report = json!({
            "version": "2.1.0",
            "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
            "runs": [{
                "tool": {
                    "driver": {
                        "name": "emmylua_check",
                        "version": "0.8.2",
                        "informationUri": "https://github.com/CppCXY/emmylua-analyzer-rust"
                    }
                },
                "results": self.sarif_runs
            }]
        });

        let pretty_json = serde_json::to_string_pretty(&sarif_report).unwrap();

        if let Some(output) = self.output.as_mut() {
            output.write_all(pretty_json.as_bytes()).unwrap();
        } else {
            println!("{}", pretty_json);
        }
    }
}

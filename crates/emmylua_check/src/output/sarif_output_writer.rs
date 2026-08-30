use std::{collections::HashMap, fs::File, io::Write};

use emmylua_code_analysis::{FileId, SalsaDatabase, file_path_to_uri};
use lsp_types::{Diagnostic, DiagnosticSeverity};
use serde_json::{Value, json};

use crate::cmd_args::OutputDestination;

use super::OutputWriter;

const CRATE_NAME: &str = env!("CARGO_PKG_NAME");
const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug)]
pub struct SarifOutputWriter {
    output: Option<File>,
    tools: HashMap<String, Value>,
    current_results: Vec<Value>,
}

impl SarifOutputWriter {
    pub fn new(output: OutputDestination) -> Self {
        let output = match output {
            OutputDestination::Stdout => None,
            OutputDestination::File(path) => {
                if let Some(parent) = path.parent()
                    && !parent.exists()
                {
                    std::fs::create_dir_all(parent).unwrap();
                }
                Some(std::fs::File::create(path).unwrap())
            }
        };

        SarifOutputWriter {
            output,
            tools: HashMap::new(),
            current_results: Vec::new(),
        }
    }

    fn get_sarif_level(&self, severity: Option<DiagnosticSeverity>) -> &'static str {
        match severity {
            Some(DiagnosticSeverity::ERROR) => "error",
            Some(DiagnosticSeverity::WARNING) => "warning",
            Some(DiagnosticSeverity::INFORMATION) => "note",
            Some(DiagnosticSeverity::HINT) => "note",
            None => "note",
            _ => "note", // Handle other possible values
        }
    }

    fn ensure_tool(&mut self) -> String {
        let tool_name = "emmylua_check".to_string();
        if !self.tools.contains_key(&tool_name) {
            let tool = json!({
                "name": CRATE_NAME,
                "version": CRATE_VERSION,
                "informationUri": "https://github.com/EmmyLuaLs/emmylua-analyzer-rust",
                "organization": "EmmyLuaLs"
            });
            self.tools.insert(tool_name.clone(), tool);
        }
        tool_name
    }

    fn convert_diagnostic_to_sarif_result(
        &mut self,
        file_uri: &str,
        diagnostic: &Diagnostic,
    ) -> Value {
        // Convert LSP Range to SARIF region
        let region = json!({
            "startLine": diagnostic.range.start.line + 1,  // SARIF uses 1-based line numbers
            "startColumn": diagnostic.range.start.character + 1,  // SARIF uses 1-based column numbers
            "endLine": diagnostic.range.end.line + 1,
            "endColumn": diagnostic.range.end.character + 1
        });

        let location = json!({
            "physicalLocation": {
                "artifactLocation": {
                    "uri": file_uri
                },
                "region": region
            }
        });

        let rule_id = diagnostic
            .code
            .as_ref()
            .map(|code| match code {
                lsp_types::NumberOrString::Number(n) => n.to_string(),
                lsp_types::NumberOrString::String(s) => s.clone(),
            })
            .unwrap_or_else(|| "unknown".to_string());

        let result = json!({
            "ruleId": rule_id,
            "level": self.get_sarif_level(diagnostic.severity),
            "message": {
                "text": diagnostic.message
            },
            "locations": [location]
        });

        result
    }
}

impl OutputWriter for SarifOutputWriter {
    fn write(&mut self, db: &SalsaDatabase, file_id: FileId, diagnostics: Vec<Diagnostic>) {
        if diagnostics.is_empty() {
            return;
        }

        let file_path = db.file_path(file_id).unwrap();
        let file_uri = file_path_to_uri(&file_path).unwrap().as_str().to_string();
        self.ensure_tool();

        for diagnostic in diagnostics {
            let result = self.convert_diagnostic_to_sarif_result(&file_uri, &diagnostic);
            self.current_results.push(result);
        }
    }

    fn finish(&mut self) {
        // Create the tool object
        let tool_name = self.ensure_tool();
        let tool = self.tools.get(&tool_name).unwrap().clone();

        // Create a single run
        let run = json!({
            "tool": {
                "driver": tool
            },
            "results": self.current_results
        });

        // Create the complete SARIF document
        let sarif_document = json!({
            "version": "2.1.0",
            "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
            "runs": [run]
        });

        let pretty_json = serde_json::to_string_pretty(&sarif_document).unwrap();

        if let Some(output) = self.output.as_mut() {
            output.write_all(pretty_json.as_bytes()).unwrap();
        } else {
            println!("{}", pretty_json);
        }
    }
}

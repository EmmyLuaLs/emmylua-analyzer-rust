use std::{fs::File, io::Write};

use emmylua_code_analysis::{FileId, SalsaDatabase};
use lsp_types::Diagnostic;
use serde_json::{Value, json};

use crate::cmd_args::OutputDestination;

use super::OutputWriter;

#[derive(Debug)]
pub struct JsonOutputWriter {
    output: Option<File>,
    first_write: bool,
    json_file_caches: Vec<Value>,
}

impl JsonOutputWriter {
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
        JsonOutputWriter {
            output,
            first_write: true,
            json_file_caches: Vec::new(),
        }
    }
}

impl OutputWriter for JsonOutputWriter {
    fn write(&mut self, db: &SalsaDatabase, file_id: FileId, diagnostics: Vec<Diagnostic>) {
        let file_path = db.file_path(file_id).unwrap();
        let file_path = file_path.to_str().unwrap();
        let mut json_diagnostics = Vec::new();
        for diagnostic in diagnostics {
            let json_diagnostic = serde_json::to_value(diagnostic).unwrap();
            json_diagnostics.push(json_diagnostic);
        }
        let json_file = json!({
            "file": file_path,
            "diagnostics": json_diagnostics,
        });

        if self.output.is_none() {
            if self.first_write {
                self.first_write = false;
                println!("[");
            } else {
                println!(",");
            }
            println!("{}", serde_json::to_string_pretty(&json_file).unwrap());
        } else {
            self.json_file_caches.push(json_file);
        }
    }

    fn finish(&mut self) {
        if let Some(output) = self.output.as_mut() {
            let pretty_json = serde_json::to_string_pretty(&self.json_file_caches).unwrap();
            output.write_all(pretty_json.as_bytes()).unwrap();
        } else if !self.first_write {
            println!("\n]");
        }
    }
}

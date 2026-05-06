use std::io::{self, Write};

#[derive(Debug, Default)]
pub struct Diagnostics {
    warnings: Vec<String>,
}

impl Diagnostics {
    pub fn duplicate_locale_key(&mut self, file: &str, key: &str) {
        self.warn(format!(
            "{file}: duplicate locale key `{key}` (kept first, ignored later entry)"
        ));
    }

    pub fn missing_replacement_target(&mut self, file: &str, key: &str) {
        self.warn(format!("{file}: missing replacement target for `{key}`"));
    }

    pub fn emit(&self) {
        let mut stderr = io::stderr();
        for warning in &self.warnings {
            let _ = writeln!(stderr, "warning: {warning}");
        }
    }

    fn warn(&mut self, message: String) {
        if !self.warnings.iter().any(|existing| existing == &message) {
            self.warnings.push(message);
        }
    }
}

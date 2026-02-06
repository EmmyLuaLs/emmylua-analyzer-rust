use std::fmt::Write;

pub struct EmmyLuaEmitter {
    output: String,
    write_private: bool,
}

impl EmmyLuaEmitter {
    pub fn new(write_private: bool) -> Self {
        Self {
            output: String::new(),
            write_private,
        }
    }

    /// Write a raw line.
    pub fn write_line(&mut self, line: &str) {
        self.output.push_str(line);
        self.output.push('\n');
    }

    /// Write an empty line.
    pub fn blank_line(&mut self) {
        self.output.push('\n');
    }

    /// Write a doc comment line: `--- text`.
    pub fn write_doc_comment(&mut self, text: &str) {
        for line in text.lines() {
            let _ = writeln!(self.output, "--- {}", line);
        }
    }

    /// Write `---@class ClassName`.
    pub fn write_class(&mut self, name: &str) {
        let _ = writeln!(
            self.output,
            "---@class{} {}",
            if self.write_private { "(private)" } else { "" },
            name
        );
    }

    /// Write `---@class ClassName : ParentClass`.
    #[allow(dead_code)]
    pub fn write_class_extends(&mut self, name: &str, parent: &str) {
        let _ = writeln!(
            self.output,
            "---@class{} {} : {}",
            if self.write_private { "(private)" } else { "" },
            name,
            parent
        );
    }

    /// Write `---@field name type` with description on a separate line above.
    /// If the field name contains special characters (like `$`), wraps it in `["name"]`.
    pub fn write_field(&mut self, name: &str, ty: &str, description: Option<&str>) {
        // Emit description above the field
        if let Some(desc) = description {
            for line in desc.lines() {
                let _ = writeln!(self.output, "--- {}", line);
            }
        }

        // Use ["name"] form for field names with special characters
        let formatted_name = if needs_bracket_notation(name) {
            format!("[\"{}\"]", name)
        } else {
            name.to_string()
        };

        let _ = writeln!(self.output, "---@field {} {}", formatted_name, ty);
    }

    /// Write `---@alias AliasName`.
    pub fn write_alias_header(&mut self, name: &str) {
        let _ = writeln!(
            self.output,
            "---@alias{} {}",
            if self.write_private { "(private)" } else { "" },
            name
        );
    }

    /// Write `---| "value" # description`.
    pub fn write_alias_variant(&mut self, value: &str, description: Option<&str>) {
        match description {
            Some(desc) => {
                let _ = writeln!(self.output, "---| \"{}\" # {}", value, desc);
            }
            None => {
                let _ = writeln!(self.output, "---| \"{}\"", value);
            }
        }
    }

    /// Write `---| type # description` (for non-string union members).
    pub fn write_alias_type_variant(&mut self, ty: &str, description: Option<&str>) {
        match description {
            Some(desc) => {
                let _ = writeln!(self.output, "---| {} # {}", ty, desc);
            }
            None => {
                let _ = writeln!(self.output, "---| {}", ty);
            }
        }
    }

    /// Write a local variable declaration to anchor the class.
    pub fn write_local_placeholder(&mut self, name: &str) {
        let _ = writeln!(self.output, "local {} = {{}}", name);
    }

    /// Consume and return the final output string.
    pub fn finish(self) -> String {
        self.output
    }
}

/// Check if a field name needs bracket notation (contains special characters).
fn needs_bracket_notation(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    // Must start with letter or underscore
    let first = name.chars().next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return true;
    }
    // Remaining must be alphanumeric or underscore
    !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

use serde_json::Value;

/// Walks a JSON Schema and resolves `$ref` references.
pub struct SchemaWalker<'a> {
    root: &'a Value,
}

impl<'a> SchemaWalker<'a> {
    pub fn new(root: &'a Value) -> Self {
        Self { root }
    }

    /// Extract the type name from a `$ref` string like `#/$defs/FooBar` → `FooBar`.
    pub fn ref_type_name(ref_str: &str) -> Option<&str> {
        ref_str.rsplit('/').next()
    }

    /// Get all definitions from `$defs`.
    pub fn get_definitions(&self) -> Vec<(&'a str, &'a Value)> {
        let mut defs = Vec::new();
        if let Some(obj) = self.root.get("$defs").and_then(|v| v.as_object()) {
            for (name, schema) in obj {
                defs.push((name.as_str(), schema));
            }
        }
        defs
    }

    /// Get the root schema title (used as the root class name).
    pub fn root_title(&self) -> Option<&'a str> {
        self.root.get("title").and_then(|v| v.as_str())
    }

    /// Get the root schema object.
    pub fn root_schema(&self) -> &'a Value {
        self.root
    }
}

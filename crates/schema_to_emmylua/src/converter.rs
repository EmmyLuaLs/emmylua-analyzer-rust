use serde_json::Value;

use crate::lua_emitter::EmmyLuaEmitter;
use crate::markdown_doc::sanitize_description;
use crate::schema_walker::SchemaWalker;

/// Converts a JSON Schema document into LuaLS (EmmyLua) annotation strings.
pub struct SchemaConverter {
    /// If true, emit `local X = {}` after each class definition so that
    /// the annotations have a symbol to attach to.
    pub emit_local_placeholders: bool,
    /// Prefix to add to all class and alias names (e.g. "schema.").
    pub type_prefix: String,

    pub is_private: bool,
}

impl SchemaConverter {
    pub fn new(is_private: bool) -> Self {
        Self {
            emit_local_placeholders: false,
            type_prefix: "schema.".to_string(),
            is_private,
        }
    }

    /// Convert a JSON Schema (as `serde_json::Value`) into LuaLS annotation text.
    pub fn convert(&self, schema: &Value) -> String {
        let walker = SchemaWalker::new(schema);
        let mut emitter = EmmyLuaEmitter::new(self.is_private);

        // Header
        emitter.write_line("--- This file was auto-generated from JSON Schema.");
        emitter.write_line("--- Do not edit manually.");
        emitter.blank_line();

        // First, process all $defs (aliases and classes)
        let defs = walker.get_definitions();

        // Separate into enums/aliases and object classes so we emit aliases first
        let mut alias_defs = Vec::new();
        let mut class_defs = Vec::new();

        for (name, def_schema) in &defs {
            if self.is_enum_or_alias(def_schema) {
                alias_defs.push((*name, *def_schema));
            } else {
                class_defs.push((*name, *def_schema));
            }
        }

        // Emit aliases (enums)
        for (name, def_schema) in &alias_defs {
            let prefixed = format!("{}{}", self.type_prefix, name);
            self.emit_definition(&walker, &mut emitter, &prefixed, def_schema);
            emitter.blank_line();
        }

        // Emit classes from $defs
        for (name, def_schema) in &class_defs {
            let prefixed = format!("{}{}", self.type_prefix, name);
            self.emit_definition(&walker, &mut emitter, &prefixed, def_schema);
            emitter.blank_line();
        }

        // Emit the root schema as a class
        if let Some(title) = walker.root_title() {
            let root = walker.root_schema();
            if root.get("properties").is_some() {
                let prefixed = format!("{}{}", self.type_prefix, title);
                self.emit_object_class(&walker, &mut emitter, &prefixed, root);
                emitter.blank_line();
            }
        }

        emitter.finish()
    }

    /// Convert a JSON Schema string into LuaLS annotation text.
    pub fn convert_from_str(&self, json_str: &str) -> Result<String, serde_json::Error> {
        let schema: Value = serde_json::from_str(json_str)?;
        Ok(self.convert(&schema))
    }

    // ── Internal helpers ──────────────────────────────────────────────

    /// Check if a schema definition is an enum/alias (not an object class).
    fn is_enum_or_alias(&self, schema: &Value) -> bool {
        // Has `enum` array
        if schema.get("enum").and_then(|v| v.as_array()).is_some() {
            return true;
        }
        // Has `oneOf` with const values
        if let Some(one_of) = schema.get("oneOf").and_then(|v| v.as_array()) {
            if one_of
                .iter()
                .all(|item| item.get("const").is_some() || item.get("enum").is_some())
            {
                return true;
            }
        }
        // Has `anyOf` but is not object-like (union of types)
        if schema.get("anyOf").is_some() && schema.get("properties").is_none() {
            return true;
        }
        false
    }

    /// Emit a single definition (`$defs` entry).
    fn emit_definition(
        &self,
        walker: &SchemaWalker,
        emitter: &mut EmmyLuaEmitter,
        name: &str,
        schema: &Value,
    ) {
        // Determine kind
        if let Some(enum_values) = schema.get("enum").and_then(|v| v.as_array()) {
            // Description for non-object types (object classes handle their own description)
            if let Some(desc) = schema.get("description").and_then(|v| v.as_str()) {
                emitter.write_doc_comment(&sanitize_description(desc));
            }
            // Simple enum: ---@alias Name "v1" | "v2" | ...
            self.emit_enum_alias(emitter, name, enum_values);
        } else if let Some(one_of) = schema.get("oneOf").and_then(|v| v.as_array()) {
            if let Some(desc) = schema.get("description").and_then(|v| v.as_str()) {
                emitter.write_doc_comment(&sanitize_description(desc));
            }
            // oneOf with const values → alias
            if one_of
                .iter()
                .all(|item| item.get("const").is_some() || item.get("enum").is_some())
            {
                self.emit_one_of_alias(emitter, name, one_of);
            } else {
                // oneOf with mixed types → alias with type variants
                self.emit_one_of_type_alias(walker, emitter, name, one_of);
            }
        } else if schema.get("anyOf").is_some() && schema.get("properties").is_none() {
            if let Some(desc) = schema.get("description").and_then(|v| v.as_str()) {
                emitter.write_doc_comment(&sanitize_description(desc));
            }
            self.emit_any_of_alias(walker, emitter, name, schema);
        } else if schema.get("properties").is_some()
            || schema.get("type").and_then(|v| v.as_str()) == Some("object")
        {
            // Object → class (emit_object_class handles its own description)
            self.emit_object_class(walker, emitter, name, schema);
        } else {
            // Fallback: emit as alias to the resolved type
            if let Some(desc) = schema.get("description").and_then(|v| v.as_str()) {
                emitter.write_doc_comment(&sanitize_description(desc));
            }
            let ty = self.resolve_type(walker, schema);
            emitter.write_alias_header(name);
            emitter.write_alias_type_variant(&ty, None);
        }
    }

    /// Emit `---@alias Name "v1" | "v2" | ...` from a simple `enum` array.
    fn emit_enum_alias(&self, emitter: &mut EmmyLuaEmitter, name: &str, values: &[Value]) {
        emitter.write_alias_header(name);
        for val in values {
            if let Some(s) = val.as_str() {
                emitter.write_alias_variant(s, None);
            }
        }
    }

    /// Emit `---@alias Name` from `oneOf` with `const` values.
    fn emit_one_of_alias(&self, emitter: &mut EmmyLuaEmitter, name: &str, one_of: &[Value]) {
        emitter.write_alias_header(name);
        for item in one_of {
            let const_val = item.get("const").and_then(|v| v.as_str()).or_else(|| {
                item.get("enum")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_str())
            });
            let desc = item.get("description").and_then(|v| v.as_str());
            if let Some(val) = const_val {
                emitter.write_alias_variant(val, desc);
            }
        }
    }

    /// Emit `---@alias Name` from `oneOf` with mixed type variants (not all const).
    fn emit_one_of_type_alias(
        &self,
        walker: &SchemaWalker,
        emitter: &mut EmmyLuaEmitter,
        name: &str,
        one_of: &[Value],
    ) {
        emitter.write_alias_header(name);
        for item in one_of {
            let desc = item.get("description").and_then(|v| v.as_str());
            let ty = self.resolve_type(walker, item);
            emitter.write_alias_type_variant(&ty, desc);
        }
    }

    /// Emit `---@alias Name` from `anyOf`.
    fn emit_any_of_alias(
        &self,
        walker: &SchemaWalker,
        emitter: &mut EmmyLuaEmitter,
        name: &str,
        schema: &Value,
    ) {
        if let Some(any_of) = schema.get("anyOf").and_then(|v| v.as_array()) {
            emitter.write_alias_header(name);
            for item in any_of {
                let desc = item.get("description").and_then(|v| v.as_str());
                // Skip null entries (they make the whole type nullable)
                if item.get("type").and_then(|v| v.as_str()) == Some("null") {
                    continue;
                }
                let ty = self.resolve_type(walker, item);
                emitter.write_alias_type_variant(&ty, desc);
            }
        }
    }

    /// Emit `---@class Name` with `---@field` entries.
    fn emit_object_class(
        &self,
        walker: &SchemaWalker,
        emitter: &mut EmmyLuaEmitter,
        name: &str,
        schema: &Value,
    ) {
        // Description
        if let Some(desc) = schema.get("description").and_then(|v| v.as_str()) {
            emitter.write_doc_comment(&sanitize_description(desc));
        }

        emitter.write_class(name);

        // Required fields set
        let required: Vec<&str> = schema
            .get("required")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        // Properties → fields
        if let Some(props) = schema.get("properties").and_then(|v| v.as_object()) {
            for (field_name, field_schema) in props {
                let desc = field_schema.get("description").and_then(|v| v.as_str());
                let is_required = required.contains(&field_name.as_str());
                let ty = self.resolve_field_type(walker, field_schema, !is_required);
                emitter.write_field(field_name, &ty, desc);
            }
        }

        // additionalProperties → index signature
        if let Some(additional) = schema.get("additionalProperties") {
            if additional.is_object() {
                let value_ty = self.resolve_type(walker, additional);
                let index_ty = format!("[string] : {}", value_ty);
                emitter.write_field(&index_ty, "", Some("Additional properties"));
            }
        }

        if self.emit_local_placeholders {
            emitter.write_local_placeholder(name);
        }
    }

    /// Resolve a field's type, handling nullable and optional.
    fn resolve_field_type(&self, walker: &SchemaWalker, schema: &Value, optional: bool) -> String {
        let mut ty = self.resolve_type(walker, schema);

        // Check if nullable from type array like ["string", "null"]
        let is_nullable = self.is_nullable(schema);

        if is_nullable || optional {
            if !ty.ends_with('?') {
                ty.push('?');
            }
        }

        ty
    }

    /// Check if a schema is nullable.
    fn is_nullable(&self, schema: &Value) -> bool {
        // type: ["string", "null"]
        if let Some(arr) = schema.get("type").and_then(|v| v.as_array()) {
            return arr.iter().any(|t| t.as_str() == Some("null"));
        }
        // anyOf with null
        if let Some(any_of) = schema.get("anyOf").and_then(|v| v.as_array()) {
            return any_of
                .iter()
                .any(|item| item.get("type").and_then(|v| v.as_str()) == Some("null"));
        }
        // oneOf with null
        if let Some(one_of) = schema.get("oneOf").and_then(|v| v.as_array()) {
            return one_of
                .iter()
                .any(|item| item.get("type").and_then(|v| v.as_str()) == Some("null"));
        }
        false
    }

    #[allow(clippy::only_used_in_recursion)]
    /// Resolve a schema node into a LuaLS type string.
    fn resolve_type(&self, walker: &SchemaWalker, schema: &Value) -> String {
        // $ref → type name with prefix
        if let Some(ref_str) = schema.get("$ref").and_then(|v| v.as_str()) {
            let name = SchemaWalker::ref_type_name(ref_str).unwrap_or("any");
            return format!("{}{}", self.type_prefix, name);
        }

        // anyOf → union type (excluding null)
        if let Some(any_of) = schema.get("anyOf").and_then(|v| v.as_array()) {
            let types: Vec<String> = any_of
                .iter()
                .filter(|item| item.get("type").and_then(|v| v.as_str()) != Some("null"))
                .map(|item| self.resolve_type(walker, item))
                .collect();
            let has_null = any_of
                .iter()
                .any(|item| item.get("type").and_then(|v| v.as_str()) == Some("null"));
            let mut result = types.join(" | ");
            if has_null {
                result.push('?');
            }
            return result;
        }

        // oneOf → check if it's a string enum or union
        if let Some(one_of) = schema.get("oneOf").and_then(|v| v.as_array()) {
            let types: Vec<String> = one_of
                .iter()
                .filter(|item| item.get("type").and_then(|v| v.as_str()) != Some("null"))
                .map(|item| {
                    if let Some(const_val) = item.get("const").and_then(|v| v.as_str()) {
                        format!("\"{}\"", const_val)
                    } else {
                        self.resolve_type(walker, item)
                    }
                })
                .collect();
            return types.join(" | ");
        }

        // type field
        if let Some(type_val) = schema.get("type") {
            // Array type: ["string", "null"]
            if let Some(arr) = type_val.as_array() {
                let types: Vec<String> = arr
                    .iter()
                    .filter_map(|t| t.as_str())
                    .filter(|t| *t != "null")
                    .map(|t| self.json_type_to_lua(t))
                    .collect();
                let has_null = arr.iter().any(|t| t.as_str() == Some("null"));
                let mut result = types.join(" | ");
                if has_null {
                    result.push('?');
                }
                return result;
            }

            // Simple type
            if let Some(type_str) = type_val.as_str() {
                match type_str {
                    "array" => {
                        // Resolve items type
                        let item_type = if let Some(items) = schema.get("items") {
                            self.resolve_type(walker, items)
                        } else {
                            "any".to_string()
                        };
                        return format!("{}[]", item_type);
                    }
                    "object" => {
                        // Object with additionalProperties
                        if let Some(additional) = schema.get("additionalProperties") {
                            if additional.is_object() {
                                let value_ty = self.resolve_type(walker, additional);
                                return format!("table<string, {}>", value_ty);
                            }
                        }
                        return "table".to_string();
                    }
                    other => {
                        return self.json_type_to_lua(other);
                    }
                }
            }
        }

        // enum (string values only)
        if let Some(enum_values) = schema.get("enum").and_then(|v| v.as_array()) {
            let variants: Vec<String> = enum_values
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| format!("\"{}\"", s))
                .collect();
            return variants.join(" | ");
        }

        // const
        if let Some(const_val) = schema.get("const").and_then(|v| v.as_str()) {
            return format!("\"{}\"", const_val);
        }

        "any".to_string()
    }

    /// Map JSON Schema primitive type names to Lua type names.
    fn json_type_to_lua(&self, json_type: &str) -> String {
        match json_type {
            "string" => "string".to_string(),
            "integer" => "integer".to_string(),
            "number" => "number".to_string(),
            "boolean" => "boolean".to_string(),
            "null" => "nil".to_string(),
            "object" => "table".to_string(),
            "array" => "any[]".to_string(),
            _ => "any".to_string(),
        }
    }
}

impl Default for SchemaConverter {
    fn default() -> Self {
        Self::new(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn converter() -> SchemaConverter {
        let mut c = SchemaConverter::new(false);
        c.type_prefix = "schema.".to_string();
        c
    }

    #[test]
    fn test_simple_object() {
        let schema = json!({
            "title": "Config",
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The name"
                },
                "count": {
                    "type": "integer",
                    "description": "Item count"
                }
            },
            "required": ["name"]
        });

        let output = converter().convert(&schema);
        assert!(output.contains("---@class schema.Config"));
        assert!(output.contains("--- The name\n---@field name string"));
        assert!(output.contains("--- Item count\n---@field count integer?"));
    }

    #[test]
    fn test_enum_alias() {
        let schema = json!({
            "title": "Root",
            "type": "object",
            "properties": {},
            "$defs": {
                "Color": {
                    "type": "string",
                    "enum": ["red", "green", "blue"]
                }
            }
        });

        let output = converter().convert(&schema);
        assert!(output.contains("---@alias schema.Color"));
        assert!(output.contains("---| \"red\""));
        assert!(output.contains("---| \"green\""));
        assert!(output.contains("---| \"blue\""));
    }

    #[test]
    fn test_one_of_with_const() {
        let schema = json!({
            "title": "Root",
            "type": "object",
            "properties": {},
            "$defs": {
                "Level": {
                    "oneOf": [
                        { "type": "string", "const": "error", "description": "Error level" },
                        { "type": "string", "const": "warning", "description": "Warning level" }
                    ]
                }
            }
        });

        let output = converter().convert(&schema);
        assert!(output.contains("---@alias schema.Level"));
        assert!(output.contains("---| \"error\" # Error level"));
        assert!(output.contains("---| \"warning\" # Warning level"));
    }

    #[test]
    fn test_ref_field() {
        let schema = json!({
            "title": "Root",
            "type": "object",
            "properties": {
                "level": {
                    "$ref": "#/$defs/Level",
                    "default": "info"
                }
            },
            "$defs": {
                "Level": {
                    "oneOf": [
                        { "type": "string", "const": "info" },
                        { "type": "string", "const": "error" }
                    ]
                }
            }
        });

        let output = converter().convert(&schema);
        assert!(output.contains("---@field level schema.Level?"));
    }

    #[test]
    fn test_nullable_type() {
        let schema = json!({
            "title": "Config",
            "type": "object",
            "properties": {
                "name": {
                    "type": ["string", "null"]
                }
            }
        });

        let output = converter().convert(&schema);
        assert!(output.contains("---@field name string?"));
    }

    #[test]
    fn test_array_field() {
        let schema = json!({
            "title": "Config",
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            }
        });

        let output = converter().convert(&schema);
        assert!(output.contains("---@field items string[]?"));
    }

    #[test]
    fn test_additional_properties_map() {
        let schema = json!({
            "title": "Config",
            "type": "object",
            "properties": {
                "settings": {
                    "type": "object",
                    "additionalProperties": {
                        "$ref": "#/$defs/Level"
                    }
                }
            },
            "$defs": {
                "Level": {
                    "oneOf": [
                        { "type": "string", "const": "info" },
                        { "type": "string", "const": "error" }
                    ]
                }
            }
        });

        let output = converter().convert(&schema);
        assert!(output.contains("---@field settings table<string, schema.Level>?"));
    }

    #[test]
    fn test_any_of_alias() {
        let schema = json!({
            "title": "Root",
            "type": "object",
            "properties": {},
            "$defs": {
                "Item": {
                    "anyOf": [
                        { "type": "string", "description": "Simple string path" },
                        { "$ref": "#/$defs/ItemConfig", "description": "Config object" }
                    ]
                },
                "ItemConfig": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }
            }
        });

        let output = converter().convert(&schema);
        assert!(output.contains("---@alias schema.Item"));
        assert!(output.contains("---| string # Simple string path"));
        assert!(output.contains("---| schema.ItemConfig # Config object"));
    }

    #[test]
    fn test_special_field_name() {
        let schema = json!({
            "title": "Config",
            "type": "object",
            "properties": {
                "$schema": {
                    "type": ["string", "null"]
                }
            }
        });

        let output = converter().convert(&schema);
        assert!(output.contains("---@field [\"$schema\"] string?"));
    }

    #[test]
    fn test_description_above_field() {
        let schema = json!({
            "title": "Config",
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The name of the config"
                }
            },
            "required": ["name"]
        });

        let output = converter().convert(&schema);
        // Description must be on the line directly above the field
        assert!(output.contains("--- The name of the config\n---@field name string\n"));
    }
}

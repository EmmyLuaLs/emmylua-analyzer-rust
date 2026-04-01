use super::{
    completion_builder::CompletionBuilder,
    providers::{
        desc_provider, doc_name_token_provider, doc_tag_provider, doc_type_provider,
        file_path_provider, module_path_provider, table_field_provider,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionContext {
    DocTag,
    DocName,
    DocType,
    DocDescription,
    ModulePath,
    FilePath,
    TableField,
    General,
}

impl CompletionContext {
    pub fn analyze(builder: &CompletionBuilder) -> Self {
        if doc_tag_provider::can_add_completion(builder) {
            return Self::DocTag;
        }

        if doc_name_token_provider::can_add_completion(builder) {
            return Self::DocName;
        }

        if doc_type_provider::get_completion_type(builder).is_some() {
            return Self::DocType;
        }

        if desc_provider::can_add_completion(builder) {
            return Self::DocDescription;
        }

        if module_path_provider::can_add_completion(builder) {
            return Self::ModulePath;
        }

        if file_path_provider::can_add_completion(builder) {
            return Self::FilePath;
        }

        if table_field_provider::has_exclusive_completion(builder) {
            return Self::TableField;
        }

        Self::General
    }
}

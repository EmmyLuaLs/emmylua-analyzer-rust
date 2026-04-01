use super::{
    completion_builder::CompletionBuilder,
    providers::{
        CompletionProvider, DescProvider, DocNameTokenProvider, DocTagProvider, DocTypeProvider,
        FilePathProvider, ModulePathProvider, TableFieldProvider,
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
        if DocTagProvider.supports(builder) {
            return Self::DocTag;
        }

        if DocNameTokenProvider.supports(builder) {
            return Self::DocName;
        }

        if DocTypeProvider.supports(builder) {
            return Self::DocType;
        }

        if DescProvider.supports(builder) {
            return Self::DocDescription;
        }

        if ModulePathProvider.supports(builder) {
            return Self::ModulePath;
        }

        if FilePathProvider.supports(builder) {
            return Self::FilePath;
        }

        if TableFieldProvider.supports(builder) {
            return Self::TableField;
        }

        Self::General
    }
}

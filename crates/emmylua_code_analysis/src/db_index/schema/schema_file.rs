use crate::FileId;

#[derive(Debug, Clone)]
pub enum JsonSchemaFile {
    NeedResolve,
    Resolved(FileId),
}

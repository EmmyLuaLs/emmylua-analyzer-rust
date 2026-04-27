use crate::{FileId, SalsaSummaryHost, Vfs};

use super::{CompilationModuleIndex, CompilationTypeIndex};

#[derive(Clone, Copy)]
pub struct CompilationIndexContext<'a> {
    pub summary: &'a SalsaSummaryHost,
    pub vfs: &'a Vfs,
    pub modules: Option<&'a CompilationModuleIndex>,
    pub types: Option<&'a CompilationTypeIndex>,
}

impl<'a> CompilationIndexContext<'a> {
    pub fn new(summary: &'a SalsaSummaryHost, vfs: &'a Vfs) -> Self {
        Self {
            summary,
            vfs,
            modules: None,
            types: None,
        }
    }

    pub fn with_modules(self, modules: &'a CompilationModuleIndex) -> Self {
        Self {
            modules: Some(modules),
            ..self
        }
    }

    pub fn with_types(self, types: &'a CompilationTypeIndex) -> Self {
        Self {
            types: Some(types),
            ..self
        }
    }
}

pub trait FileBackedIndex {
    fn remove_file(&mut self, file_id: FileId);

    fn rebuild_file(&mut self, ctx: &CompilationIndexContext<'_>, file_id: FileId);

    fn clear(&mut self);

    fn sync_file(&mut self, ctx: &CompilationIndexContext<'_>, file_id: FileId) {
        self.remove_file(file_id);
        self.rebuild_file(ctx, file_id);
    }
}

use std::collections::HashMap;

use emmylua_code_analysis::{SalsaDatabase, TypeDef};
use lsp_types::Uri;

use super::rename_decl::push_edit;
use crate::handlers::common::type_def_rename_ranges;

#[allow(clippy::mutable_key_type)]
pub fn rename_type_references(
    salsa: &SalsaDatabase,
    def: &TypeDef,
    new_name: String,
    result: &mut HashMap<Uri, HashMap<lsp_types::Range, String>>,
) -> Option<()> {
    // Definition sites + use sites (replacing display name / full-name tail).
    let ranges = type_def_rename_ranges(salsa, def, &new_name);
    for (file_id, range, text) in ranges {
        push_edit(salsa, file_id, range, text, result);
    }
    Some(())
}

use std::collections::HashMap;

use emmylua_code_analysis::{SalsaDatabase, SemanticId};
use lsp_types::Uri;

use super::rename_decl::push_edit;
use crate::handlers::common::member_key_rename_ranges;

#[allow(clippy::mutable_key_type)]
pub fn rename_member_references(
    salsa: &SalsaDatabase,
    member: &SemanticId,
    new_name: String,
    result: &mut HashMap<Uri, HashMap<lsp_types::Range, String>>,
) -> Option<()> {
    // Member definition sites + index key sites with the same key text, limited to the member's declaration file (mirrors old origin-owner semantics).
    let SemanticId::Member(key) = member else {
        return None;
    };
    let ranges = member_key_rename_ranges(salsa, member, &new_name);
    for (file_id, range, text) in ranges {
        if file_id != key.file_id {
            continue;
        }
        push_edit(salsa, file_id, range, text, result);
    }
    Some(())
}

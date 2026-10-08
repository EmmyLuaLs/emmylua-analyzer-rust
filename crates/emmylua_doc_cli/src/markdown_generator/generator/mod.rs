mod global_gen;
mod index_gen;
mod mod_gen;
mod typ_gen;

use crate::doc_model::DocProperty;
pub use global_gen::generate_global_markdown;
pub use index_gen::generate_index;
pub use mod_gen::generate_module_markdown;
pub use typ_gen::generate_type_markdown;

use super::markdown_types::Property;

pub(crate) fn collect_property(doc_property: &DocProperty) -> Property {
    Property {
        deprecated: doc_property.deprecated.then(|| "Deprecated".to_string()),
        ..Default::default()
    }
}

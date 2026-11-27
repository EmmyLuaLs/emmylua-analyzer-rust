mod global_gen;
mod index_gen;
mod mod_gen;
mod typ_gen;

use crate::doc_links::LinkContext;
use crate::doc_model::DocProperty;
pub use global_gen::generate_global_markdown;
pub use index_gen::generate_index;
pub use mod_gen::generate_module_markdown;
pub use typ_gen::generate_type_markdown;

use super::markdown_types::Property;

pub(crate) fn collect_property(doc_property: &DocProperty) -> Property {
    Property {
        description: doc_property.description.clone(),
        deprecated: doc_property.deprecated.then(|| "Deprecated".to_string()),
        ..Default::default()
    }
}

pub(crate) fn collect_property_with_links(
    doc_property: &DocProperty,
    links: Option<&LinkContext>,
) -> Property {
    let mut property = collect_property(doc_property);
    if let Some(links) = links {
        property.description = property
            .description
            .map(|description| links.rewrite_markdown(&description));
        property.see = property.see.map(|see| links.rewrite_markdown(&see));
        property.other = property.other.map(|other| links.rewrite_markdown(&other));
    }
    property
}

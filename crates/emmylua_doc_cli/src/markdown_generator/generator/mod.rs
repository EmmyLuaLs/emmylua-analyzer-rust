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
    let mut property = Property {
        description: doc_property.description.clone(),
        deprecated: doc_property.deprecated.then(|| {
            doc_property
                .deprecation_reason
                .clone()
                .unwrap_or_else(|| "Deprecated".to_string())
        }),
        ..Default::default()
    };
    for (tag_name, content) in &doc_property.tags {
        let (target, line) = match tag_name.as_str() {
            "see" => (&mut property.see, content.clone()),
            _ => (&mut property.other, format!("@{} {}", tag_name, content)),
        };
        let text = target.get_or_insert_with(String::new);
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&line);
    }
    property
}

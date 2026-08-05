mod global_gen;
mod index_gen;
mod mod_gen;
mod typ_gen;

use emmylua_code_analysis::{DbIndex, LuaDeprecated, LuaSemanticDeclId};
pub use global_gen::generate_global_markdown;
pub use index_gen::generate_index;
pub use mod_gen::generate_module_markdown;
pub use typ_gen::generate_type_markdown;

use super::markdown_types::Property;

pub(crate) fn collect_property(db: &DbIndex, semantic_decl: LuaSemanticDeclId) -> Property {
    let mut doc_property = Property::default();
    let property = db.get_property_index().get_property(&semantic_decl);
    if let Some(property) = property {
        if let Some(description) = property.description() {
            doc_property.description = Some(description.clone());
        }

        if let Some(deprecated) = property.deprecated() {
            match deprecated {
                LuaDeprecated::Deprecated => {
                    doc_property.deprecated = Some("Deprecated".to_string())
                }
                LuaDeprecated::DeprecatedWithMessage(message) => {
                    doc_property.deprecated = Some(message.to_string())
                }
            }
        }

        if let Some(tag_content) = property.tag_content() {
            for (tag_name, content) in tag_content.get_all_tags() {
                match tag_name.as_str() {
                    "see" => {
                        let see_content = doc_property.see.get_or_insert_with(String::new);
                        if !see_content.is_empty() {
                            see_content.push('\n');
                        }
                        see_content.push_str(content);
                    }
                    _ => {
                        let other_content = doc_property.other.get_or_insert_with(String::new);
                        if !other_content.is_empty() {
                            other_content.push('\n');
                        }
                        other_content.push_str(&format!("@{} {}", tag_name, content));
                    }
                }
            }
        }
    }

    doc_property
}

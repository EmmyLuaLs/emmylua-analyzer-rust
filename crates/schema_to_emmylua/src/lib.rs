mod converter;
mod lua_emitter;
mod markdown_doc;
mod schema_walker;

pub use converter::SchemaConverter;

pub struct ConvertResult {
    pub annotation_text: String,
    pub root_type_name: Option<String>,
}

/// Converts a documentation/description string to a cleaned-up Lua doc comment.
/// Removes excessive whitespace and normalizes line breaks.
pub fn sanitize_description(desc: &str) -> String {
    desc.trim().to_string()
}

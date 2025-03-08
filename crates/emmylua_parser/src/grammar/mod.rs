use crate::{parser::CompleteMarker, parser_error::LuaParseError};

mod doc;
mod lua;

type ParseResult = Result<CompleteMarker, LuaParseError>;
pub use doc::parse_comment;
pub use lua::parse_chunk;

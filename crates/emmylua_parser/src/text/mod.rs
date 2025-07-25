mod line_index;
mod reader;
mod test;
mod text_range;

pub use line_index::LineIndex;
pub use reader::{Reader, ReaderWithMarks};
pub(crate) use text_range::SourceRange;

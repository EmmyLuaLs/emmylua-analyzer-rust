//! Git-style unified diff rendering for formatter `--check` output.
//!
//! This module is intentionally self-contained so the diff rendering can be
//! polished without touching the formatting pipeline.

mod color;
mod sha1;
#[cfg(test)]
mod test;
mod unified;

pub use color::Colorizer;
pub use sha1::git_blob_hash;
pub use unified::{DiffRenderOptions, render_unified_diff};

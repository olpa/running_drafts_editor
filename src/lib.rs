//! Reusable implementation behind the line-oriented Running Drafts Editor.

pub mod backend;
pub mod chunking;
pub mod document;
pub mod navigation;
pub mod persistence;
pub mod project;
pub mod transcription;

#[cfg(test)]
extern crate self as running_drafts_editor;
pub mod replay;
pub mod session;
#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod test_support;

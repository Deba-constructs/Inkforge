//! Edit history.
//!
//! Painting goes into layers, so pixel edits record the tiles they touched plus
//! the layer they belong to. Structural changes (add, delete, reorder) and layer
//! property changes are their own variants, so one undo step means one action.

pub mod undo;

pub use undo::{Edit, History, Recorder, DEFAULT_MEMORY_LIMIT, TILE};

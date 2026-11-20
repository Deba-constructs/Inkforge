//! Layer model and stack.
//!
//! A document owns a [`LayerStack`]: an ordered set of layers plus a cached
//! composite used for display. Each layer owns its own RGBA [`Surface`], so
//! painting, undo and the eyedropper all target one layer at a time.
//!
//! [`Surface`]: crate::engine::Surface

pub mod group;
pub mod layer;
pub mod stack;

pub use group::{Group, GroupProps};
pub use layer::{composite_pixel, BlendMode, Layer, LayerId, LayerProps};
pub use stack::{Entry, FlattenedStack, LayerStack, MergedLayer};

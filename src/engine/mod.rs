//! Painting engine: the document model, the raster surface and the maths the UI
//! needs to show them.
//!
//! Milestone 2 adds real brush stamping ([`Surface`]). Milestone 4 adds the
//! layer stack ([`crate::layers`]), which owns the surfaces and supplies the
//! composite the canvas draws; Milestone 7 draws that composite from a tile
//! pyramid ([`TilePyramid`]) so only visible tiles reach the GPU.

pub mod brush;
pub mod canvas;
pub mod fill;
pub(crate) mod pool;
pub mod presets;
pub mod pen;
pub mod pyramid;
pub mod renderer;
pub mod resize;
pub mod selection;
pub mod shape;
pub mod stabilizer;
pub mod surface;
pub mod transform;
pub mod viewport;

pub use brush::{
    BrushDab, BrushSettings, MAX_SIZE, MAX_SPACING, MIN_DAB_RADIUS, MIN_SIZE, MIN_SPACING,
};
pub use pen::{PenSample, PenSettings, PenTip};
pub use presets::{BrushPreset, PRESETS};
pub use stabilizer::Stabilizer;
pub use canvas::{
    validate_canvas_size, Background, Canvas, DEFAULT_DPI, MAX_CANVAS_DIM, MAX_CANVAS_PIXELS,
    MAX_DPI, MAX_PIXEL_ASPECT, MIN_CANVAS_DIM, MIN_PIXEL_ASPECT,
};
pub use fill::FillMask;
pub use pyramid::{TilePyramid, TILE};
pub use renderer::ThumbnailCache;
pub use resize::Anchor;
pub use selection::{
    Clip, Selection, SelectionCombine, SelectionMode, SoftSpan, MAX_FEATHER, MAX_GROW,
};
pub use shape::{shape_path, ShapeKind};
pub use surface::{segment_bounds, PixelRegion, Surface};
pub use transform::Transform;
pub use viewport::Viewport;

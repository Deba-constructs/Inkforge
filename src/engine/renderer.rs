//! Cached textures for the parts of a document that are not the composite.
//!
//! The composite itself is drawn through the tile pyramid in
//! [`crate::engine::pyramid`]; this module covers the small previews the layer
//! list needs.

use std::collections::HashMap;

use crate::egui::{ColorImage, Context, TextureHandle, TextureId, TextureOptions};
use crate::layers::LayerId;

use super::Surface;

/// Longest edge of a layer thumbnail, in pixels.
pub const THUMBNAIL_MAX: u32 = 28;

/// Downscaled copy of a surface, keeping its aspect ratio inside `max_edge`.
///
/// Nearest-neighbour sampling is deliberate: a thumbnail only has to show where
/// the paint is, and this keeps it cheap on huge canvases.
pub fn thumbnail_image(surface: &Surface, max_edge: u32) -> (ColorImage, [u32; 2]) {
    let width = surface.width().max(1);
    let height = surface.height().max(1);
    let max_edge = max_edge.max(1);
    let scale = (max_edge as f32 / width.max(height) as f32).min(1.0);

    let thumb_width = ((width as f32 * scale).round() as u32).max(1);
    let thumb_height = ((height as f32 * scale).round() as u32).max(1);

    let mut pixels = Vec::with_capacity((thumb_width * thumb_height) as usize);
    for y in 0..thumb_height {
        let source_y = (y * height / thumb_height).min(height - 1);
        for x in 0..thumb_width {
            let source_x = (x * width / thumb_width).min(width - 1);
            pixels.push(surface.pixel(source_x, source_y));
        }
    }

    (
        ColorImage::new(
            [thumb_width as usize, thumb_height as usize],
            pixels,
        ),
        [thumb_width, thumb_height],
    )
}

struct CachedThumbnail {
    revision: u64,
    size: [u32; 2],
    texture: TextureHandle,
}

/// Per-layer thumbnail textures.
///
/// A thumbnail is only rebuilt when its layer's pixels change, so an idle layer
/// list costs nothing beyond the draw calls.
#[derive(Default)]
pub struct ThumbnailCache {
    entries: HashMap<LayerId, CachedThumbnail>,
}

impl ThumbnailCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Texture for one layer, rebuilt when its pixels changed.
    pub fn texture_id(&mut self, ctx: &Context, id: LayerId, surface: &Surface) -> TextureId {
        let stale = match self.entries.get(&id) {
            Some(cached) => cached.revision != surface.revision(),
            None => true,
        };

        if stale {
            let (image, size) = thumbnail_image(surface, THUMBNAIL_MAX);
            match self.entries.get_mut(&id) {
                Some(cached) => {
                    cached.texture.set(image, TextureOptions::LINEAR);
                    cached.revision = surface.revision();
                    cached.size = size;
                }
                None => {
                    let texture = ctx.load_texture(
                        format!("inkforge-layer-thumb-{id}"),
                        image,
                        TextureOptions::LINEAR,
                    );
                    self.entries.insert(
                        id,
                        CachedThumbnail {
                            revision: surface.revision(),
                            size,
                            texture,
                        },
                    );
                }
            }
        }

        self.entries
            .get(&id)
            .expect("thumbnail is created above when missing")
            .texture
            .id()
    }

    /// Drop textures for layers that no longer exist.
    pub fn prune(&mut self, alive: &[LayerId]) {
        self.entries.retain(|id, _| alive.contains(id));
    }
}

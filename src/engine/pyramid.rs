//! Tile pyramid for the canvas composite.
//!
//! A single full-resolution texture is the obvious way to show the composite,
//! and it is the wrong one for a large document: every frame has to hand the
//! GPU an image that can be hundreds of megabytes, large documents hit the
//! driver's maximum texture size, and zooming out asks the sampler to squash
//! millions of texels into a few hundred screen pixels.
//!
//! This module keeps the composite as a pyramid of half-resolution levels, each
//! split into fixed 256 × 256 tiles. Only the tiles that intersect the visible
//! part of the page are ever uploaded or drawn, and the level is chosen so one
//! texel is roughly one screen pixel. Panning therefore costs the newly exposed
//! tiles instead of the whole canvas, and a 10 000 px document drawn at 12%
//! uses a handful of 64×-downscaled tiles instead of one 400 MB texture.
//!
//! Levels are built lazily from the CPU composite, so there is never a second
//! full-resolution copy in memory, and a tile is rebuilt only when the pixels
//! under it changed (the surface's dirty rectangle drives that).

use std::collections::HashMap;

use crate::egui::{
    epaint::{Mesh, Vertex},
    Color32, ColorImage, Context, Painter, Pos2, Rect, Shape, TextureFilter, TextureHandle,
    TextureId, TextureOptions, Vec2,
};

use super::{PixelRegion, Surface};

/// Edge of one tile at full resolution, in canvas pixels.
///
/// 256 is large enough that a 1080p canvas is 40 tiles (so per-tile overhead
/// stays negligible) and small enough that repainting one tile while brushing
/// never touches more than a few hundred kilobytes.
pub const TILE: u32 = 256;

/// Most levels a document gets: full resolution plus seven halvings.
const MAX_LEVELS: usize = 8;

/// Tiles kept per level before off-screen textures are released.
///
/// Eight levels × 128 tiles × 256 KiB ≈ 256 MB in the worst case, and the
/// visible working set is always kept because eviction only drops tiles that
/// were not drawn in the latest frame.
const TILES_PER_LEVEL: usize = 128;

/// Samples taken along each axis when a coarse texel is built.
///
/// A 32×-downscaled texel covers 1 024 source pixels; averaging all of them
/// would make the first zoom-out pause for no visible gain at that size, so the
/// box filter caps at 4 × 4 evenly spread samples.
const MAX_TAPS: u32 = 4;

/// Rotate a vector by `angle` radians (screen space, y down).
fn rotate(vector: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(
        vector.x * cos - vector.y * sin,
        vector.x * sin + vector.y * cos,
    )
}

/// The four screen corners of `rect` (page space) turned about the page centre.
fn turned_corners(page: Rect, rect: Rect, rotation: f32) -> [Pos2; 4] {
    let centre = page.center();
    [
        rect.min,
        Pos2::new(rect.max.x, rect.min.y),
        rect.max,
        Pos2::new(rect.min.x, rect.max.y),
    ]
    .map(|corner| centre + rotate(corner - centre, rotation))
}

/// Draw one tile as a textured quad, so rotation needs no separate texture or
/// per-angle bitmap cache: the same uploaded tile serves every angle.
fn draw_textured_quad(painter: &Painter, texture: TextureId, corners: [Pos2; 4]) {
    let uvs = [
        Pos2::new(0.0, 0.0),
        Pos2::new(1.0, 0.0),
        Pos2::new(1.0, 1.0),
        Pos2::new(0.0, 1.0),
    ];

    let mut mesh = Mesh::default();
    mesh.texture_id = texture;
    for (pos, uv) in corners.into_iter().zip(uvs) {
        mesh.vertices.push(Vertex {
            pos,
            uv,
            color: Color32::WHITE,
        });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(Shape::mesh(mesh));
}

fn texture_options() -> TextureOptions {
    // Crisp pixels when magnified (painting at 800% must not look blurry) and
    // smooth filtering between texels when minified.
    TextureOptions {
        magnification: TextureFilter::Nearest,
        minification: TextureFilter::Linear,
        ..TextureOptions::default()
    }
}

/// What one [`TilePyramid::draw`] call did. Used by tests and diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TileStats {
    /// Pyramid level drawn from; 0 is full resolution.
    pub level: u32,
    /// Tiles that intersected the visible page.
    pub tiles_drawn: usize,
    /// Tiles whose pixels had to be sent to the GPU.
    pub tiles_uploaded: usize,
    /// Total texels uploaded this frame.
    pub pixels_uploaded: usize,
}

struct TileTexture {
    texture: TextureHandle,
    /// Draw pass that last used this tile, for eviction.
    used: u64,
}

/// What a freshly built tile covered.
struct TileBuild {
    /// Canvas pixels the tile spans.
    canvas: PixelRegion,
    /// Texels actually uploaded.
    texels: usize,
}

struct Level {
    /// Canvas pixels per texel: 1, 2, 4, …
    scale: u32,
    tiles: HashMap<(u32, u32), TileTexture>,
}

impl Level {
    /// Canvas pixels one tile of this level covers.
    ///
    /// A coarse tile is the same 256 texels wide as a full-resolution one, so
    /// it covers more of the canvas. That keeps the number of tiles on screen
    /// — and therefore the number of draw calls — independent of the zoom.
    fn span(&self) -> u32 {
        TILE * self.scale
    }

    /// Forget the tiles that overlap `region` (canvas pixels).
    fn invalidate(&mut self, region: PixelRegion) {
        if region.is_empty() {
            return;
        }
        let span = self.span();
        let first_row = region.y / span;
        let last_row = (region.y + region.height).div_ceil(span).saturating_sub(1);
        let first_col = region.x / span;
        let last_col = (region.x + region.width).div_ceil(span).saturating_sub(1);

        for ty in first_row..=last_row {
            for tx in first_col..=last_col {
                self.tiles.remove(&(tx, ty));
            }
        }
    }
}

/// Per-level tiles of the composite, uploaded on demand.
#[derive(Default)]
pub struct TilePyramid {
    width: u32,
    height: u32,
    /// Composite revision the cached tiles match.
    revision: u64,
    /// 1, 2, 4, … canvas pixels per texel, index 0 = full resolution.
    scales: Vec<u32>,
    levels: Vec<Level>,
    /// Monotonic draw counter, used to tell which tiles a frame needed.
    tick: u64,
    uploads: u64,
    last_stats: TileStats,
    /// Canvas-pixel rectangle covered by the most recent frame that uploaded
    /// anything, so callers can report how little was copied.
    last_upload: Option<PixelRegion>,
    /// Texels sent to the GPU by that same frame.
    last_upload_texels: usize,
}

impl TilePyramid {
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            revision: u64::MAX,
            scales: Vec::new(),
            levels: Vec::new(),
            tick: 0,
            uploads: 0,
            last_stats: TileStats::default(),
            last_upload: None,
            last_upload_texels: 0,
        }
    }

    /// How many pyramid levels the current document has.
    pub fn level_count(&self) -> usize {
        self.levels.len()
    }

    /// Canvas pixels per texel at `level`.
    pub fn scale(&self, level: usize) -> u32 {
        self.scales.get(level).copied().unwrap_or(1)
    }

    /// Statistics for the most recent draw.
    pub fn last_stats(&self) -> TileStats {
        self.last_stats
    }

    /// Rectangle uploaded by the most recent frame that uploaded anything.
    pub fn last_upload(&self) -> Option<PixelRegion> {
        self.last_upload
    }

    /// Texels uploaded by the most recent frame that uploaded anything.
    pub fn last_upload_texels(&self) -> usize {
        self.last_upload_texels
    }

    /// Total tile uploads since launch, for diagnostics and tests.
    pub fn uploads(&self) -> u64 {
        self.uploads
    }

    /// Pyramid level to draw at for a zoom factor.
    ///
    /// The level is chosen so one texel covers between half a screen pixel and
    /// a whole one: enough detail for the zoom, without sampling texels the
    /// screen could never show.
    pub fn level_for_zoom(zoom: f32) -> u32 {
        if !(zoom.is_finite() && zoom > 0.0) || zoom >= 0.5 {
            return 0;
        }
        let ratio = 0.5 / zoom;
        (ratio.log2().ceil() as u32).min(MAX_LEVELS as u32 - 1)
    }

    /// Draw the visible page from the pyramid, uploading missing tiles.
    ///
    /// `page` is the page rectangle in screen coordinates and `zoom` the canvas
    /// pixels per screen pixel, both from [`crate::engine::Viewport`].
    pub fn draw(
        &mut self,
        ctx: &Context,
        painter: &Painter,
        page: Rect,
        surface: &mut Surface,
        zoom: f32,
    ) -> TileStats {
        self.draw_view(ctx, painter, page, surface, zoom, 0.0)
    }

    /// Draw the page again with the whole view turned by `rotation` radians
    /// about the page centre, as the canvas view does for canvas rotation.
    ///
    /// Each tile becomes a textured quad whose corners are turned with it, so a
    /// rotated canvas is drawn at the same level of detail and from the same
    /// uploaded tiles as an upright one; only the geometry changes.
    pub fn draw_view(
        &mut self,
        ctx: &Context,
        painter: &Painter,
        page: Rect,
        surface: &mut Surface,
        zoom: f32,
        rotation: f32,
    ) -> TileStats {
        self.last_stats = TileStats::default();
        if !(zoom.is_finite() && zoom > 0.0)
            || !rotation.is_finite()
            || page.width() <= 0.0
            || page.height() <= 0.0
        {
            return self.last_stats;
        }

        self.sync(surface);
        if self.levels.is_empty() || self.width == 0 || self.height == 0 {
            return self.last_stats;
        }

        let clip = painter.clip_rect();
        let visible = page.intersect(clip);
        let turned = rotation.abs() > 1.0e-4;
        if !turned && (visible.width() <= 0.0 || visible.height() <= 0.0) {
            return self.last_stats;
        }

        let level_index = (Self::level_for_zoom(zoom) as usize).min(self.levels.len() - 1);
        let level = level_index as u32;
        self.tick = self.tick.wrapping_add(1);
        let tick = self.tick;

        // Visible area in canvas pixels. A turned view sees a diamond-shaped
        // part of the page, so its bounding box is used; that can cover a few
        // tiles that never show, which is cheaper than clipping every tile.
        let (left, top, right, bottom) = if turned {
            let centre = page.center();
            let size = page.size() / zoom;
            let mut min = Vec2::new(f32::INFINITY, f32::INFINITY);
            let mut max = Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
            for corner in [clip.min, clip.right_top(), clip.max, clip.left_bottom()] {
                let offset = rotate(corner - centre, -rotation) / zoom + size * 0.5;
                min = min.min(offset);
                max = max.max(offset);
            }
            (
                min.x.floor().max(0.0),
                min.y.floor().max(0.0),
                max.x.ceil().min(self.width as f32),
                max.y.ceil().min(self.height as f32),
            )
        } else {
            (
                ((visible.min.x - page.min.x) / zoom).floor().max(0.0),
                ((visible.min.y - page.min.y) / zoom).floor().max(0.0),
                ((visible.max.x - page.min.x) / zoom)
                    .ceil()
                    .min(self.width as f32),
                ((visible.max.y - page.min.y) / zoom)
                    .ceil()
                    .min(self.height as f32),
            )
        };
        if right <= left || bottom <= top {
            return self.last_stats;
        }

        let span = TILE * self.scales[level_index];
        let first_col = (left as u32) / span;
        let last_col = (right.max(1.0) as u32 - 1) / span;
        let first_row = (top as u32) / span;
        let last_row = (bottom.max(1.0) as u32 - 1) / span;

        let mut stats = TileStats {
            level,
            ..TileStats::default()
        };
        let mut uploaded: Option<PixelRegion> = None;

        for row in first_row..=last_row {
            for col in first_col..=last_col {
                let x0 = col * span;
                let y0 = row * span;
                let x1 = (x0 + span).min(self.width);
                let y1 = (y0 + span).min(self.height);
                if x1 <= x0 || y1 <= y0 {
                    continue;
                }

                let rect = Rect::from_min_size(
                    page.min + Vec2::new(x0 as f32 * zoom, y0 as f32 * zoom),
                    Vec2::new((x1 - x0) as f32 * zoom, (y1 - y0) as f32 * zoom),
                );
                if turned {
                    // Cull against the screen bounds of the turned tile.
                    if !Rect::from_points(&turned_corners(page, rect, rotation)).intersects(clip) {
                        continue;
                    }
                } else if !rect.intersects(visible) {
                    continue;
                }

                let (texture, built) =
                    self.tile_texture(ctx, level_index, col, row, surface, tick);
                if let Some(built) = built {
                    stats.tiles_uploaded += 1;
                    stats.pixels_uploaded += built.texels;
                    uploaded = Some(match uploaded {
                        Some(existing) => union(existing, built.canvas),
                        None => built.canvas,
                    });
                }

                if turned {
                    draw_textured_quad(painter, texture, turned_corners(page, rect, rotation));
                } else {
                    painter.image(
                        texture,
                        rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                stats.tiles_drawn += 1;
            }
        }

        if let Some(region) = uploaded {
            self.uploads = self.uploads.saturating_add(stats.tiles_uploaded as u64);
            self.last_upload = Some(region);
            self.last_upload_texels = stats.pixels_uploaded;
        }
        self.evict(tick);
        self.last_stats = stats;
        stats
    }

    /// Rebuild the level list and drop stale tiles for a changed composite.
    fn sync(&mut self, surface: &mut Surface) {
        if surface.width() != self.width || surface.height() != self.height {
            self.width = surface.width();
            self.height = surface.height();
            self.rebuild();
            self.revision = surface.revision();
            surface.clear_dirty();
            return;
        }

        if surface.revision() == self.revision {
            return;
        }

        match surface.dirty() {
            Some(region) => {
                let region = clamp(region, self.width, self.height);
                for level in &mut self.levels {
                    level.invalidate(region);
                }
            }
            // No recorded region: correctness before speed.
            None => {
                for level in &mut self.levels {
                    level.tiles.clear();
                }
            }
        }

        self.revision = surface.revision();
        surface.clear_dirty();
    }

    fn rebuild(&mut self) {
        self.levels.clear();
        self.scales.clear();
        self.last_upload = None;
        self.last_upload_texels = 0;
        if self.width == 0 || self.height == 0 {
            return;
        }

        let mut scale = 1u32;
        for index in 0..MAX_LEVELS {
            self.scales.push(scale);
            self.levels.push(Level {
                scale,
                tiles: HashMap::new(),
            });

            // Stop once the whole document fits in a single tile at this
            // level: a coarser one could only be smaller than a tile.
            let level_width = self.width.div_ceil(scale);
            let level_height = self.height.div_ceil(scale);
            if level_width <= TILE && level_height <= TILE {
                break;
            }
            if index + 1 == MAX_LEVELS {
                break;
            }
            scale *= 2;
        }
    }

    /// Texture for one tile, building and uploading it when it is missing.
    ///
    /// Returns the id and, when the tile was just uploaded, what it covered.
    fn tile_texture(
        &mut self,
        ctx: &Context,
        level_index: usize,
        col: u32,
        row: u32,
        surface: &Surface,
        tick: u64,
    ) -> (TextureId, Option<TileBuild>) {
        if let Some(tile) = self
            .levels
            .get_mut(level_index)
            .and_then(|level| level.tiles.get_mut(&(col, row)))
        {
            tile.used = tick;
            return (tile.texture.id(), None);
        }

        let scale = self.scales[level_index];
        let span = TILE * scale;
        let x0 = col * span;
        let y0 = row * span;
        let x1 = (x0 + span).min(self.width);
        let y1 = (y0 + span).min(self.height);
        let texels_w = (x1 - x0).div_ceil(scale).max(1);
        let texels_h = (y1 - y0).div_ceil(scale).max(1);

        let mut pixels = Vec::with_capacity((texels_w * texels_h) as usize);
        for texel_y in 0..texels_h {
            let block_y = y0 + texel_y * scale;
            let block_bottom = (block_y + scale).min(y1);
            for texel_x in 0..texels_w {
                let block_x = x0 + texel_x * scale;
                let block_right = (block_x + scale).min(x1);
                pixels.push(average_region(
                    surface,
                    PixelRegion {
                        x: block_x,
                        y: block_y,
                        width: block_right - block_x,
                        height: block_bottom - block_y,
                    },
                ));
            }
        }

        let image = ColorImage::new([texels_w as usize, texels_h as usize], pixels);
        let texture = ctx.load_texture(
            format!("inkforge-tile-{level_index}-{col}-{row}"),
            image,
            texture_options(),
        );
        let id = texture.id();

        if let Some(level) = self.levels.get_mut(level_index) {
            level.tiles.insert(
                (col, row),
                TileTexture {
                    texture,
                    used: tick,
                },
            );
        }

        (
            id,
            Some(TileBuild {
                canvas: PixelRegion {
                    x: x0,
                    y: y0,
                    width: x1 - x0,
                    height: y1 - y0,
                },
                texels: (texels_w * texels_h) as usize,
            }),
        )
    }

    /// Release textures for tiles no longer on screen once a level is over
    /// budget. Tiles used by the frame that just drew are always kept.
    fn evict(&mut self, tick: u64) {
        for level in &mut self.levels {
            if level.tiles.len() > TILES_PER_LEVEL {
                level.tiles.retain(|_, tile| tile.used == tick);
            }
        }
    }
}

/// Box-filtered average of a rectangle of the surface, as premultiplied RGBA.
///
/// Premultiplied channels average directly, which is exactly what a mipmap
/// level is, so downscaled tiles keep soft edges and transparency correct.
/// At most [`MAX_TAPS`] samples are taken per axis.
pub fn average_region(surface: &Surface, region: PixelRegion) -> Color32 {
    if region.is_empty() {
        return Color32::TRANSPARENT;
    }

    let taps_x = region.width.min(MAX_TAPS);
    let taps_y = region.height.min(MAX_TAPS);
    let mut sums = [0u32; 4];

    for tap_y in 0..taps_y {
        let y = region.y + tap_y * region.height / taps_y;
        for tap_x in 0..taps_x {
            let x = region.x + tap_x * region.width / taps_x;
            let pixel = surface.pixel(x, y);
            sums[0] += pixel.r() as u32;
            sums[1] += pixel.g() as u32;
            sums[2] += pixel.b() as u32;
            sums[3] += pixel.a() as u32;
        }
    }

    let count = taps_x * taps_y;
    let channel = |sum: u32| ((sum + count / 2) / count) as u8;
    Color32::from_rgba_premultiplied(
        channel(sums[0]),
        channel(sums[1]),
        channel(sums[2]),
        channel(sums[3]),
    )
}

fn union(a: PixelRegion, b: PixelRegion) -> PixelRegion {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);
    PixelRegion {
        x,
        y,
        width: right - x,
        height: bottom - y,
    }
}

fn clamp(region: PixelRegion, width: u32, height: u32) -> PixelRegion {
    let x = region.x.min(width);
    let y = region.y.min(height);
    PixelRegion {
        x,
        y,
        width: region.width.min(width - x),
        height: region.height.min(height - y),
    }
}

//! Canvas presets, measurement units and the new-canvas setup model.
//!
//! The catalog mirrors what artists expect from Photoshop, Krita and Clip Studio
//! Paint: illustration sizes, manga and webtoon pages, ISO and US print sizes,
//! social-media formats, animation frames, game assets and interface mockups.
//!
//! Everything here is plain data plus arithmetic on it, so the window that shows
//! presets and the code that creates a document share one source of truth:
//!
//! * [`CanvasSetup`] — the editable configuration (size, unit, resolution,
//!   background, bleed, safe area, pixel aspect, sketch layer).
//! * [`CanvasPreset`] — one named size a preset card stands for.
//! * [`PresetLibrary`] — the built-in catalog plus the artist's own presets,
//!   favourites and recently used documents, persisted as JSON.
//!
//! Print presets are defined in millimetres or inches and converted to pixels at
//! 300 dpi with a single rounding step, which is exactly how Photoshop sizes
//! them (A4 becomes 2480 × 3508 px, US Letter 2550 × 3300 px). The conversion
//! lives in [`unit_to_px`] so the "New canvas" window and the tests always agree.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::egui::Color32;
use crate::engine::{
    validate_canvas_size, Background, Canvas, MAX_CANVAS_DIM, MAX_DPI, MAX_PIXEL_ASPECT,
    MIN_CANVAS_DIM, MIN_PIXEL_ASPECT,
};

/// File name of the preset library inside INKFORGE's data folder.
pub const LIBRARY_FILE: &str = "canvas-presets.json";
/// Library file format version.
pub const LIBRARY_VERSION: u32 = 1;
/// How many recently used presets the library remembers.
pub const MAX_RECENT: usize = 10;
/// Print presets are resolved at this resolution unless the artist changes it.
pub const PRINT_DPI: f32 = 300.0;
/// Resolution used for screen-oriented presets (web, video, UI, game UI).
pub const SCREEN_DPI: f32 = 72.0;

/// Millimetres in one inch, exactly.
pub const MM_PER_INCH: f64 = 25.4;

// ---- categories ----------------------------------------------------------

/// A group of presets, one entry per sidebar section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Illustration,
    Comic,
    Print,
    Photo,
    Animation,
    Game,
    Design,
    /// Presets the artist saved.
    Custom,
}

impl Category {
    /// Every category, in sidebar order.
    pub const ALL: [Self; 8] = [
        Self::Illustration,
        Self::Comic,
        Self::Print,
        Self::Photo,
        Self::Animation,
        Self::Game,
        Self::Design,
        Self::Custom,
    ];

    /// Sidebar heading.
    pub fn label(self) -> &'static str {
        match self {
            Self::Illustration => "Illustration",
            Self::Comic => "Comic & Manga",
            Self::Print => "Print & Publishing",
            Self::Photo => "Photography",
            Self::Animation => "Animation",
            Self::Game => "Game Art",
            Self::Design => "Design & UI",
            Self::Custom => "My presets",
        }
    }

    /// One-line description shown above the preset cards.
    pub fn hint(self) -> &'static str {
        match self {
            Self::Illustration => "Paintings, character sheets and splash art.",
            Self::Comic => "Manga pages, webtoon strips and storyboards.",
            Self::Print => "ISO and US paper at 300 dpi, with bleed available.",
            Self::Photo => "Social posts, covers and wallpapers.",
            Self::Animation => "Frame sizes for HD to 4K animation and sprite sheets.",
            Self::Game => "Characters, textures, environments and icons.",
            Self::Design => "Desktop, tablet and mobile interface mockups.",
            Self::Custom => "Presets you saved. Edit or delete them here.",
        }
    }

    /// Index in [`Self::ALL`], used for stable sorting.
    pub fn order(self) -> usize {
        Self::ALL
            .iter()
            .position(|category| *category == self)
            .unwrap_or(Self::ALL.len())
    }
}

// ---- measurement units ---------------------------------------------------

/// The unit the size fields and rulers are shown in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    #[default]
    Pixels,
    Millimeters,
    Centimeters,
    Inches,
}

impl Unit {
    /// Every unit, in menu order.
    pub const ALL: [Self; 4] = [
        Self::Pixels,
        Self::Millimeters,
        Self::Centimeters,
        Self::Inches,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Pixels => "Pixels",
            Self::Millimeters => "Millimetres",
            Self::Centimeters => "Centimetres",
            Self::Inches => "Inches",
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Self::Pixels => "px",
            Self::Millimeters => "mm",
            Self::Centimeters => "cm",
            Self::Inches => "in",
        }
    }

    /// Decimal places that make sense for a field in this unit.
    pub fn decimals(self) -> usize {
        match self {
            Self::Pixels => 0,
            Self::Millimeters | Self::Centimeters | Self::Inches => 2,
        }
    }

    /// How much one step of a drag field moves, in this unit.
    pub fn step(self) -> f64 {
        match self {
            Self::Pixels => 1.0,
            Self::Millimeters => 1.0,
            Self::Centimeters => 0.1,
            Self::Inches => 0.1,
        }
    }

    /// Smallest value a size field accepts, in this unit.
    pub fn min_value(self) -> f64 {
        match self {
            Self::Pixels => MIN_CANVAS_DIM as f64,
            other => px_to_unit(MIN_CANVAS_DIM, other, PRINT_DPI) as f64,
        }
    }

    /// True when the unit is physical and therefore needs the resolution.
    pub fn is_physical(self) -> bool {
        !matches!(self, Self::Pixels)
    }
}

/// Convert a pixel count into `unit` at `dpi`.
pub fn px_to_unit(px: u32, unit: Unit, dpi: f32) -> f32 {
    let dpi = if dpi.is_finite() && dpi > 0.0 {
        dpi as f64
    } else {
        1.0
    };
    let px = px as f64;
    match unit {
        Unit::Pixels => px as f32,
        Unit::Millimeters => (px / dpi * MM_PER_INCH) as f32,
        Unit::Centimeters => (px / dpi * MM_PER_INCH / 10.0) as f32,
        Unit::Inches => (px / dpi) as f32,
    }
}

/// Convert a length in `unit` into pixels at `dpi`, rounded to the nearest
/// pixel and clamped to a size the document model can hold.
///
/// At 300 dpi, 210 mm is `210 / 25.4 × 300 = 2480.31` → 2480 px, the same
/// number Photoshop reports for A4.
pub fn unit_to_px(value: f32, unit: Unit, dpi: f32) -> u32 {
    if !value.is_finite() {
        return MIN_CANVAS_DIM;
    }
    let dpi = if dpi.is_finite() && dpi > 0.0 {
        dpi as f64
    } else {
        1.0
    };
    let value = value as f64;
    let px = match unit {
        Unit::Pixels => value,
        Unit::Millimeters => value / MM_PER_INCH * dpi,
        Unit::Centimeters => value * 10.0 / MM_PER_INCH * dpi,
        Unit::Inches => value * dpi,
    };
    let rounded = px.round();
    if rounded <= 0.0 {
        MIN_CANVAS_DIM
    } else {
        rounded.min(MAX_CANVAS_DIM as f64) as u32
    }
}

/// Millimetres as pixels at a resolution (used for bleed and safe margins).
pub fn mm_to_px(mm: f32, dpi: f32) -> f32 {
    if !mm.is_finite() || mm <= 0.0 {
        return 0.0;
    }
    let dpi = if dpi.is_finite() && dpi > 0.0 {
        dpi
    } else {
        1.0
    };
    mm / MM_PER_INCH as f32 * dpi
}

/// Pixels as millimetres at a resolution.
pub fn px_to_mm(px: f32, dpi: f32) -> f32 {
    if !px.is_finite() || px <= 0.0 {
        return 0.0;
    }
    let dpi = if dpi.is_finite() && dpi > 0.0 {
        dpi
    } else {
        1.0
    };
    px / dpi * MM_PER_INCH as f32
}

// ---- background ----------------------------------------------------------

/// Background choice that survives being written to the preset library.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum BackgroundSpec {
    White,
    Black,
    Transparent,
    Custom { r: u8, g: u8, b: u8 },
}

impl BackgroundSpec {
    /// Solid black, offered next to white in the window.
    pub const BLACK: Self = Self::Black;

    pub fn to_background(self) -> Background {
        match self {
            Self::White => Background::White,
            Self::Black => Background::Custom(Color32::BLACK),
            Self::Transparent => Background::Transparent,
            Self::Custom { r, g, b } => Background::Custom(Color32::from_rgb(r, g, b)),
        }
    }

    /// Read back a document background, so a new canvas starts from what the
    /// current one uses.
    pub fn from_background(background: Background) -> Self {
        match background {
            Background::White => Self::White,
            Background::Transparent => Self::Transparent,
            Background::Custom(color) if color == Color32::BLACK => Self::Black,
            Background::Custom(color) => Self::Custom {
                r: color.r(),
                g: color.g(),
                b: color.b(),
            },
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::White => "White",
            Self::Black => "Black",
            Self::Transparent => "Transparent",
            Self::Custom { .. } => "Custom",
        }
    }
}

impl Default for BackgroundSpec {
    fn default() -> Self {
        Self::White
    }
}

// ---- presets -------------------------------------------------------------

/// One named canvas size.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanvasPreset {
    /// Stable identifier; favourites and recents refer to it.
    pub id: String,
    pub name: String,
    pub category: Category,
    /// Trim width in pixels (bleed is stored separately and added on creation).
    pub width: u32,
    pub height: u32,
    pub dpi: f32,
    #[serde(default)]
    pub pixel_aspect: f32,
    #[serde(default)]
    pub bleed_px: f32,
    #[serde(default)]
    pub safe_px: f32,
    #[serde(default)]
    pub background: BackgroundSpec,
}

impl CanvasPreset {
    /// A square-pixel preset.
    pub fn new(
        id: &str,
        name: &str,
        category: Category,
        width: u32,
        height: u32,
        dpi: f32,
    ) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            category,
            width,
            height,
            dpi,
            pixel_aspect: 1.0,
            bleed_px: 0.0,
            safe_px: 0.0,
            background: BackgroundSpec::White,
        }
    }

    /// Aspect ratio, width divided by height.
    pub fn aspect(&self) -> f32 {
        if self.height == 0 {
            1.0
        } else {
            self.width as f32 / self.height as f32
        }
    }

    /// Aspect ratio as the smallest readable pair, e.g. `16:9` or `2:3`.
    pub fn aspect_label(&self) -> String {
        let (width, height) = reduce_ratio(self.width, self.height);
        format!("{width}:{height}")
    }

    pub fn megapixels(&self) -> f32 {
        self.width as f32 * self.height as f32 / 1_000_000.0
    }

    /// Physical width and height in millimetres at the preset's resolution.
    pub fn print_size_mm(&self) -> (f32, f32) {
        let dpi = self.dpi.max(1.0);
        let aspect = if self.pixel_aspect > 0.0 {
            self.pixel_aspect
        } else {
            1.0
        };
        (
            self.width as f32 * aspect / dpi * MM_PER_INCH as f32,
            self.height as f32 / dpi * MM_PER_INCH as f32,
        )
    }

    /// `"3000 × 4000 px"`.
    pub fn dimensions_label(&self) -> String {
        format!("{} × {} px", self.width, self.height)
    }

    /// `"2:3 · 12.0 MP · 300 dpi"` — everything an artist checks before
    /// committing to a canvas.
    pub fn detail_label(&self) -> String {
        format!(
            "{} · {:.1} MP · {:.0} dpi",
            self.aspect_label(),
            self.megapixels(),
            self.dpi
        )
    }

    /// Clamp anything a hand-edited library could carry into a usable preset.
    pub fn sanitised(mut self) -> Self {
        self.width = self.width.clamp(MIN_CANVAS_DIM, MAX_CANVAS_DIM);
        self.height = self.height.clamp(MIN_CANVAS_DIM, MAX_CANVAS_DIM);
        self.dpi = if self.dpi.is_finite() {
            self.dpi.clamp(1.0, MAX_DPI)
        } else {
            PRINT_DPI
        };
        self.pixel_aspect = if self.pixel_aspect.is_finite() && self.pixel_aspect > 0.0 {
            self.pixel_aspect.clamp(MIN_PIXEL_ASPECT, MAX_PIXEL_ASPECT)
        } else {
            1.0
        };
        let limit = self.width.min(self.height) as f32 / 2.0;
        self.bleed_px = if self.bleed_px.is_finite() {
            self.bleed_px.clamp(0.0, limit)
        } else {
            0.0
        };
        self.safe_px = if self.safe_px.is_finite() {
            self.safe_px.clamp(0.0, limit)
        } else {
            0.0
        };
        self.name = self.name.trim().to_owned();
        if self.name.is_empty() {
            self.name = String::from("Custom size");
        }
        if self.id.trim().is_empty() {
            self.id = format!("custom.{}", unique_suffix());
        }
        self
    }
}

/// Reduce a pixel size to the smallest ratio with the same meaning.
///
/// Large sizes rarely reduce exactly (3000 × 4000 does), so a near miss is
/// rounded to one decimal place instead of showing `500:667`.
pub fn reduce_ratio(width: u32, height: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (1, 1);
    }

    // Find the smallest denominator whose fraction lands closest to the real
    // ratio. Exact ratios win immediately (3:4, 16:9); an odd page like
    // 2480 × 3508 becomes 12:17, which is far more useful on a card than
    // 620:877.
    let ratio = width as f32 / height as f32;
    let mut best = (width, height);
    let mut best_error = f32::MAX;
    for denominator in 1..=32u32 {
        let numerator = (ratio * denominator as f32).round().max(1.0) as u32;
        let error = (numerator as f32 / denominator as f32 - ratio).abs();
        if error < best_error - 1e-6 {
            best_error = error;
            best = (numerator, denominator);
        }
        if error < 1e-6 {
            break;
        }
    }

    let divisor = gcd(best.0, best.1);
    (best.0 / divisor, best.1 / divisor)
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let next = a % b;
        a = b;
        b = next;
    }
    a.max(1)
}

fn mm_preset(id: &str, name: &str, width_mm: f32, height_mm: f32) -> CanvasPreset {
    CanvasPreset::new(
        id,
        name,
        Category::Print,
        unit_to_px(width_mm, Unit::Millimeters, PRINT_DPI),
        unit_to_px(height_mm, Unit::Millimeters, PRINT_DPI),
        PRINT_DPI,
    )
}

fn inch_preset(id: &str, name: &str, width_in: f32, height_in: f32) -> CanvasPreset {
    CanvasPreset::new(
        id,
        name,
        Category::Print,
        unit_to_px(width_in, Unit::Inches, PRINT_DPI),
        unit_to_px(height_in, Unit::Inches, PRINT_DPI),
        PRINT_DPI,
    )
}

fn screen_preset(
    id: &str,
    name: &str,
    category: Category,
    width: u32,
    height: u32,
) -> CanvasPreset {
    CanvasPreset::new(id, name, category, width, height, SCREEN_DPI)
}

/// The built-in catalog: 53 presets across seven categories.
///
/// The list is rebuilt on demand (it is small) so it can never go stale against
/// a loaded library, and the ids are stable so favourites survive upgrades.
pub fn builtin_presets() -> Vec<CanvasPreset> {
    use Category::{Animation, Comic, Design, Game, Illustration, Photo};

    let mut presets = Vec::with_capacity(53);

    // A. Illustration
    presets.extend([
        CanvasPreset::new(
            "illustration.square",
            "Square Illustration",
            Illustration,
            2000,
            2000,
            PRINT_DPI,
        ),
        CanvasPreset::new(
            "illustration.digital",
            "Digital Illustration",
            Illustration,
            3000,
            4000,
            PRINT_DPI,
        ),
        CanvasPreset::new(
            "illustration.portrait",
            "Portrait Illustration",
            Illustration,
            2480,
            3508,
            PRINT_DPI,
        ),
        CanvasPreset::new(
            "illustration.landscape",
            "Landscape Illustration",
            Illustration,
            3508,
            2480,
            PRINT_DPI,
        ),
        CanvasPreset::new(
            "illustration.character",
            "Character Sheet",
            Illustration,
            3000,
            4000,
            PRINT_DPI,
        ),
        CanvasPreset::new(
            "illustration.concept",
            "Concept Art",
            Illustration,
            3840,
            2160,
            PRINT_DPI,
        ),
        CanvasPreset::new(
            "illustration.splash",
            "Splash Art",
            Illustration,
            4000,
            6000,
            PRINT_DPI,
        ),
    ]);

    // B. Comic & Manga
    presets.extend([
        CanvasPreset::new(
            "comic.manga-b4",
            "Japanese Manga B4",
            Comic,
            2570,
            3640,
            PRINT_DPI,
        ),
        CanvasPreset::new(
            "comic.manga-a4",
            "Japanese Manga A4",
            Comic,
            2100,
            2970,
            PRINT_DPI,
        ),
        screen_preset("comic.webtoon", "Webtoon Vertical", Comic, 800, 12800),
        CanvasPreset::new("comic.page", "Comic Page", Comic, 2550, 3300, PRINT_DPI),
        CanvasPreset::new("comic.strip", "Comic Strip", Comic, 4000, 1500, PRINT_DPI),
        CanvasPreset::new(
            "comic.spread",
            "Manga Double Spread",
            Comic,
            7280,
            5150,
            PRINT_DPI,
        ),
        screen_preset("comic.storyboard", "Storyboard", Comic, 1920, 1080),
    ]);

    // C. Print & Publishing (resolved from millimetres and inches at 300 dpi)
    presets.extend([
        mm_preset("print.a0", "A0", 841.0, 1189.0),
        mm_preset("print.a1", "A1", 594.0, 841.0),
        mm_preset("print.a2", "A2", 420.0, 594.0),
        mm_preset("print.a3", "A3", 297.0, 420.0),
        mm_preset("print.a4", "A4", 210.0, 297.0),
        mm_preset("print.a5", "A5", 148.0, 210.0),
        mm_preset("print.a6", "A6", 105.0, 148.0),
        inch_preset("print.letter", "US Letter", 8.5, 11.0),
        inch_preset("print.legal", "US Legal", 8.5, 14.0),
        inch_preset("print.tabloid", "US Tabloid", 11.0, 17.0),
        inch_preset("print.poster", "Poster", 18.0, 24.0),
    ]);

    // D. Photography & photo editing
    presets.extend([
        screen_preset(
            "photo.instagram-square",
            "Instagram Square",
            Photo,
            1080,
            1080,
        ),
        screen_preset(
            "photo.instagram-portrait",
            "Instagram Portrait",
            Photo,
            1080,
            1350,
        ),
        screen_preset(
            "photo.instagram-story",
            "Instagram Story",
            Photo,
            1080,
            1920,
        ),
        screen_preset("photo.facebook-cover", "Facebook Cover", Photo, 1640, 624),
        screen_preset(
            "photo.youtube-thumbnail",
            "YouTube Thumbnail",
            Photo,
            1280,
            720,
        ),
        screen_preset("photo.youtube-banner", "YouTube Banner", Photo, 2560, 1440),
        screen_preset(
            "photo.wallpaper-fhd",
            "Wallpaper Full HD",
            Photo,
            1920,
            1080,
        ),
        screen_preset("photo.wallpaper-2k", "Wallpaper 2K", Photo, 2560, 1440),
        screen_preset("photo.wallpaper-4k", "Wallpaper 4K", Photo, 3840, 2160),
    ]);

    // E. Animation
    presets.extend([
        screen_preset("animation.hd", "HD Animation", Animation, 1280, 720),
        screen_preset("animation.fhd", "Full HD Animation", Animation, 1920, 1080),
        screen_preset("animation.2k", "2K Animation", Animation, 2560, 1440),
        screen_preset("animation.4k", "4K Animation", Animation, 3840, 2160),
        screen_preset(
            "animation.portrait",
            "Animation Portrait",
            Animation,
            1080,
            1920,
        ),
        screen_preset(
            "animation.sprite-sheet",
            "Sprite Sheet",
            Animation,
            2048,
            2048,
        ),
    ]);

    // F. Game art & assets
    presets.extend([
        screen_preset("game.character", "Game Character", Game, 2048, 2048),
        screen_preset("game.texture", "Game Texture", Game, 1024, 1024),
        screen_preset(
            "game.texture-hires",
            "High Resolution Texture",
            Game,
            4096,
            4096,
        ),
        screen_preset("game.environment", "Environment Concept", Game, 3840, 2160),
        screen_preset("game.pixel-art", "Pixel Art", Game, 320, 320),
        screen_preset("game.icon", "Game Icon", Game, 512, 512),
        screen_preset("game.ui-asset", "UI Asset", Game, 1920, 1080),
    ]);

    // G. Design & UI
    presets.extend([
        screen_preset("design.desktop", "Desktop UI", Design, 1920, 1080),
        screen_preset("design.laptop", "Laptop UI", Design, 1440, 900),
        screen_preset("design.mobile", "Mobile UI", Design, 390, 844),
        screen_preset("design.tablet", "Tablet UI", Design, 1024, 1366),
        screen_preset("design.app-icon", "App Icon", Design, 1024, 1024),
        screen_preset("design.presentation", "Presentation", Design, 1920, 1080),
    ]);

    presets
}

// ---- the setup edited in the New canvas window ---------------------------

/// Everything the "New canvas" window can set, turned into a document by
/// [`Self::to_canvas`].
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasSetup {
    pub name: String,
    /// Trim width in pixels (before bleed).
    pub width: u32,
    /// Trim height in pixels (before bleed).
    pub height: u32,
    pub dpi: f32,
    pub unit: Unit,
    pub background: BackgroundSpec,
    pub pixel_aspect: f32,
    /// Bleed on each side, in pixels.
    pub bleed_px: f32,
    /// Safe-area margin inwards from the trim edge, in pixels.
    pub safe_px: f32,
    pub sketch_layer: bool,
    /// Keep the width/height ratio while either field is edited.
    pub aspect_lock: bool,
    /// Preset the settings came from, if any.
    pub preset_id: Option<String>,
}

impl Default for CanvasSetup {
    fn default() -> Self {
        Self {
            name: String::from("Untitled"),
            width: 1920,
            height: 1080,
            dpi: SCREEN_DPI,
            unit: Unit::Pixels,
            background: BackgroundSpec::White,
            pixel_aspect: 1.0,
            bleed_px: 0.0,
            safe_px: 0.0,
            sketch_layer: false,
            aspect_lock: false,
            preset_id: None,
        }
    }
}

impl CanvasSetup {
    /// Settings for a copy of an open document.
    pub fn from_canvas(canvas: &Canvas) -> Self {
        Self {
            name: format!("{} copy", canvas.name()),
            width: canvas.width(),
            height: canvas.height(),
            dpi: canvas.dpi(),
            unit: Unit::Pixels,
            background: BackgroundSpec::from_background(canvas.background()),
            pixel_aspect: canvas.pixel_aspect(),
            bleed_px: canvas.bleed_px(),
            safe_px: canvas.safe_px(),
            sketch_layer: false,
            aspect_lock: false,
            // A copy is not any preset: its size may have been transformed.
            preset_id: None,
        }
    }

    /// Take every value from a preset, including its bleed and background.
    pub fn apply_preset(&mut self, preset: &CanvasPreset) {
        self.width = preset.width;
        self.height = preset.height;
        self.dpi = preset.dpi;
        self.background = preset.background;
        self.pixel_aspect = if preset.pixel_aspect > 0.0 {
            preset.pixel_aspect
        } else {
            1.0
        };
        self.bleed_px = preset.bleed_px;
        self.safe_px = preset.safe_px;
        self.preset_id = Some(preset.id.clone());
    }

    /// Snapshot the current settings as a preset, keeping the bleed and safe
    /// margin so a print preset round-trips.
    pub fn to_preset(&self, id: String, name: String, category: Category) -> CanvasPreset {
        CanvasPreset {
            id,
            name,
            category,
            width: self.width,
            height: self.height,
            dpi: self.dpi,
            pixel_aspect: self.pixel_aspect,
            bleed_px: self.bleed_px,
            safe_px: self.safe_px,
            background: self.background,
        }
    }

    pub fn aspect(&self) -> f32 {
        if self.height == 0 {
            1.0
        } else {
            self.width as f32 / self.height as f32
        }
    }

    pub fn aspect_label(&self) -> String {
        let (width, height) = reduce_ratio(self.width, self.height);
        format!("{width}:{height}")
    }

    /// Set the width in pixels, honouring the aspect lock.
    pub fn set_width_px(&mut self, width: u32) {
        let width = width.clamp(MIN_CANVAS_DIM, MAX_CANVAS_DIM);
        if self.aspect_lock && self.width > 0 && self.height > 0 {
            let ratio = self.width as f64 / self.height as f64;
            self.width = width;
            self.height =
                ((width as f64 / ratio).round() as u32).clamp(MIN_CANVAS_DIM, MAX_CANVAS_DIM);
        } else {
            self.width = width;
        }
    }

    /// Set the height in pixels, honouring the aspect lock.
    pub fn set_height_px(&mut self, height: u32) {
        let height = height.clamp(MIN_CANVAS_DIM, MAX_CANVAS_DIM);
        if self.aspect_lock && self.width > 0 && self.height > 0 {
            let ratio = self.width as f64 / self.height as f64;
            self.height = height;
            self.width =
                ((height as f64 * ratio).round() as u32).clamp(MIN_CANVAS_DIM, MAX_CANVAS_DIM);
        } else {
            self.height = height;
        }
    }

    /// Force a specific ratio, keeping the current width.
    ///
    /// Used by the ratio chips (`1:1`, `4:5`, `16:9`, …). The lock stays on so
    /// the ratio survives the next edit.
    pub fn set_aspect(&mut self, numerator: u32, denominator: u32) {
        if numerator == 0 || denominator == 0 {
            return;
        }
        self.aspect_lock = true;
        self.height = ((self.width as f64 * denominator as f64 / numerator as f64).round() as u32)
            .clamp(MIN_CANVAS_DIM, MAX_CANVAS_DIM);
    }

    /// Swap width and height (the portrait/landscape switch).
    pub fn toggle_orientation(&mut self) {
        std::mem::swap(&mut self.width, &mut self.height);
    }

    pub fn is_portrait(&self) -> bool {
        self.height >= self.width
    }

    /// Width in the current unit, for the size field.
    pub fn width_value(&self) -> f32 {
        px_to_unit(self.width, self.unit, self.dpi)
    }

    pub fn height_value(&self) -> f32 {
        px_to_unit(self.height, self.unit, self.dpi)
    }

    pub fn set_width_value(&mut self, value: f32) {
        let px = unit_to_px(value, self.unit, self.dpi);
        self.set_width_px(px);
    }

    pub fn set_height_value(&mut self, value: f32) {
        let px = unit_to_px(value, self.unit, self.dpi);
        self.set_height_px(px);
    }

    /// Bleed and safe margin in millimetres, which is how print guides are set.
    pub fn bleed_mm(&self) -> f32 {
        px_to_mm(self.bleed_px, self.dpi)
    }

    pub fn safe_mm(&self) -> f32 {
        px_to_mm(self.safe_px, self.dpi)
    }

    pub fn set_bleed_mm(&mut self, mm: f32) {
        self.bleed_px = mm_to_px(mm, self.dpi);
    }

    pub fn set_safe_mm(&mut self, mm: f32) {
        self.safe_px = mm_to_px(mm, self.dpi);
    }

    /// Document size after bleed, in pixels.
    pub fn canvas_size_px(&self) -> (u32, u32) {
        let per_side = self.bleed_px.round().clamp(0.0, MAX_CANVAS_DIM as f32) as u32;
        let width = (self.width + per_side * 2).min(MAX_CANVAS_DIM);
        let height = (self.height + per_side * 2).min(MAX_CANVAS_DIM);
        (width, height)
    }

    /// Physical trim size, in millimetres.
    pub fn trim_size_mm(&self) -> (f32, f32) {
        let dpi = self.dpi.max(1.0);
        (
            self.width as f32 * self.pixel_aspect / dpi * MM_PER_INCH as f32,
            self.height as f32 / dpi * MM_PER_INCH as f32,
        )
    }

    pub fn megapixels(&self) -> f32 {
        let (width, height) = self.canvas_size_px();
        width as f32 * height as f32 / 1_000_000.0
    }

    /// Memory for one layer plus the composited image, in mebibytes.
    pub fn memory_mb(&self) -> f32 {
        let (width, height) = self.canvas_size_px();
        let bytes = width as f64 * height as f64 * 4.0 * 2.0;
        (bytes / (1024.0 * 1024.0)) as f32
    }

    /// Check the settings before building a document: returns a message the
    /// window can show under the fields.
    pub fn validate(&self) -> Result<(), String> {
        if !self.dpi.is_finite() || !(1.0..=MAX_DPI).contains(&self.dpi) {
            return Err(format!(
                "Resolution must be between 1 and {MAX_DPI:.0} dpi (got {}).",
                self.dpi
            ));
        }
        if !self.pixel_aspect.is_finite()
            || !(MIN_PIXEL_ASPECT..=MAX_PIXEL_ASPECT).contains(&self.pixel_aspect)
        {
            return Err(format!(
                "Pixel aspect ratio must be between {MIN_PIXEL_ASPECT} and {MAX_PIXEL_ASPECT}."
            ));
        }

        let limit = self.width.min(self.height) as f32 / 2.0;
        for (label, value) in [("Bleed", self.bleed_px), ("Safe area", self.safe_px)] {
            if !value.is_finite() || value < 0.0 {
                return Err(format!("{label} must be a positive length."));
            }
            if value > limit {
                return Err(format!(
                    "{label} is larger than half the canvas; reduce it."
                ));
            }
        }

        let (width, height) = self.canvas_size_px();
        validate_canvas_size(width, height)
    }

    /// Build the document these settings describe.
    pub fn to_canvas(&self) -> Result<Canvas, String> {
        self.validate()?;
        let (width, height) = self.canvas_size_px();
        let mut canvas = Canvas::try_new(
            self.name.clone(),
            width,
            height,
            self.background.to_background(),
        )?;
        canvas.set_print_setup(self.dpi, self.pixel_aspect, self.bleed_px, self.safe_px);
        if self.sketch_layer {
            canvas.add_sketch_layer();
        }
        // A new document has nothing to save yet, including its extra layer.
        canvas.mark_saved();
        Ok(canvas)
    }
}

// ---- browsing ------------------------------------------------------------

/// Which set of presets the window lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    #[default]
    All,
    Favorites,
    Recent,
}

impl Scope {
    pub const ALL: [Self; 3] = [Self::All, Self::Favorites, Self::Recent];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All presets",
            Self::Favorites => "Favorites",
            Self::Recent => "Recent",
        }
    }
}

/// Order of the preset cards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Category,
    Name,
    Size,
    RecentlyUsed,
}

impl Sort {
    pub const ALL: [Self; 4] = [Self::Category, Self::Name, Self::Size, Self::RecentlyUsed];

    pub fn label(self) -> &'static str {
        match self {
            Self::Category => "Category",
            Self::Name => "Name",
            Self::Size => "Largest first",
            Self::RecentlyUsed => "Recently used",
        }
    }
}

/// Options the window hands to [`PresetLibrary::browse`].
#[derive(Clone, Debug)]
pub struct Browse {
    pub scope: Scope,
    pub category: Option<Category>,
    pub query: String,
    pub sort: Sort,
}

impl Default for Browse {
    fn default() -> Self {
        Self {
            scope: Scope::All,
            category: None,
            query: String::new(),
            sort: Sort::Category,
        }
    }
}

// ---- the library ---------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct Stored {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    custom: Vec<CanvasPreset>,
    #[serde(default)]
    favorites: Vec<String>,
    #[serde(default)]
    recent: Vec<CanvasPreset>,
}

#[derive(Serialize, Deserialize)]
struct ExportFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    presets: Vec<CanvasPreset>,
}

/// The built-in catalog plus the artist's presets, favourites and recents.
#[derive(Clone, Debug, Default)]
pub struct PresetLibrary {
    custom: Vec<CanvasPreset>,
    favorites: Vec<String>,
    recent: Vec<CanvasPreset>,
}

impl PresetLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    /// Read the library, tolerating a missing, damaged or hand-edited file: the
    /// built-in catalog is always available, so a broken library costs the
    /// artist their own presets, never the ability to make a canvas.
    pub fn load(dir: &Path) -> Self {
        let Ok(bytes) = fs::read(dir.join(LIBRARY_FILE)) else {
            return Self::new();
        };
        let Ok(stored) = serde_json::from_slice::<Stored>(&bytes) else {
            return Self::new();
        };

        let custom: Vec<CanvasPreset> = stored
            .custom
            .into_iter()
            .map(CanvasPreset::sanitised)
            .collect();
        let mut library = Self {
            favorites: stored
                .favorites
                .into_iter()
                .filter(|id| !id.trim().is_empty())
                .collect(),
            recent: stored
                .recent
                .into_iter()
                .map(CanvasPreset::sanitised)
                .take(MAX_RECENT)
                .collect(),
            custom,
        };
        // Favourites that no longer match anything are harmless but pointless.
        let known: Vec<String> = library.all_ids();
        library.favorites.retain(|id| known.contains(id));
        library
    }

    /// Write the library, creating the data folder if needed.
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        fs::create_dir_all(dir).map_err(|error| {
            format!(
                "Could not create the preset folder {}: {error}",
                dir.display()
            )
        })?;

        let stored = Stored {
            version: LIBRARY_VERSION,
            custom: self.custom.clone(),
            favorites: self.favorites.clone(),
            recent: self.recent.clone(),
        };
        let json = serde_json::to_vec_pretty(&stored)
            .map_err(|error| format!("Could not serialise the presets: {error}"))?;
        fs::write(dir.join(LIBRARY_FILE), json).map_err(|error| {
            format!(
                "Could not write {}: {error}",
                dir.join(LIBRARY_FILE).display()
            )
        })
    }

    /// Built-ins followed by the artist's presets.
    pub fn all(&self) -> Vec<CanvasPreset> {
        let mut presets = builtin_presets();
        presets.extend(self.custom.iter().cloned());
        presets
    }

    pub fn builtin_count(&self) -> usize {
        builtin_presets().len()
    }

    pub fn custom(&self) -> &[CanvasPreset] {
        &self.custom
    }

    pub fn favorites(&self) -> &[String] {
        &self.favorites
    }

    pub fn recent(&self) -> &[CanvasPreset] {
        &self.recent
    }

    fn all_ids(&self) -> Vec<String> {
        self.all().into_iter().map(|preset| preset.id).collect()
    }

    /// Find one preset by id, built-in or custom.
    pub fn find(&self, id: &str) -> Option<CanvasPreset> {
        self.all().into_iter().find(|preset| preset.id == id)
    }

    pub fn is_favorite(&self, id: &str) -> bool {
        self.favorites.iter().any(|favorite| favorite == id)
    }

    /// Add or remove a favourite; returns whether it is one afterwards.
    pub fn toggle_favorite(&mut self, id: &str) -> bool {
        if self.is_favorite(id) {
            self.favorites.retain(|favorite| favorite != id);
            false
        } else {
            self.favorites.push(id.to_owned());
            true
        }
    }

    /// How many presets a category holds, for the sidebar count.
    pub fn count(&self, category: Category) -> usize {
        self.all()
            .into_iter()
            .filter(|preset| preset.category == category)
            .count()
    }

    /// A fresh id for a preset the artist is saving.
    pub fn next_custom_id(&self) -> String {
        let existing: Vec<String> = self.all_ids();
        loop {
            let id = format!("custom.{}", unique_suffix());
            if !existing.iter().any(|known| known == &id) {
                return id;
            }
        }
    }

    /// Insert a custom preset, replacing one with the same id.
    pub fn upsert(&mut self, preset: CanvasPreset) {
        let preset = preset.sanitised();
        match self
            .custom
            .iter_mut()
            .find(|existing| existing.id == preset.id)
        {
            Some(slot) => *slot = preset,
            None => self.custom.push(preset),
        }
    }

    /// Delete a custom preset. Built-ins cannot be deleted, only unfavourited.
    pub fn delete(&mut self, id: &str) -> bool {
        let before = self.custom.len();
        self.custom.retain(|preset| preset.id != id);
        let removed = self.custom.len() != before;
        if removed {
            self.favorites.retain(|favorite| favorite != id);
        }
        removed
    }

    /// Remember a preset as just used. Recently used documents open first.
    pub fn remember_use(&mut self, preset: CanvasPreset) {
        let preset = preset.sanitised();
        self.recent.retain(|known| known.id != preset.id);
        self.recent.insert(0, preset);
        self.recent.truncate(MAX_RECENT);
    }

    /// Forget everything the artist added: custom presets, favourites and
    /// recents. The built-in catalog is untouched because it is not stored.
    pub fn restore_defaults(&mut self) {
        self.custom.clear();
        self.favorites.clear();
        self.recent.clear();
    }

    /// The presets the window lists, filtered and ordered.
    pub fn browse(&self, options: &Browse) -> Vec<CanvasPreset> {
        let mut presets: Vec<CanvasPreset> = match options.scope {
            Scope::All => self.all(),
            Scope::Favorites => self
                .all()
                .into_iter()
                .filter(|preset| self.is_favorite(&preset.id))
                .collect(),
            Scope::Recent => self.recent.clone(),
        };

        if let Some(category) = options.category {
            presets.retain(|preset| preset.category == category);
        }

        let query = options.query.trim().to_lowercase();
        if !query.is_empty() {
            presets.retain(|preset| preset.name.to_lowercase().contains(&query));
        }

        match options.sort {
            Sort::Category => presets.sort_by(|a, b| {
                a.category
                    .order()
                    .cmp(&b.category.order())
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            }),
            Sort::Name => presets.sort_by(|a, b| {
                a.name
                    .to_lowercase()
                    .cmp(&b.name.to_lowercase())
                    .then_with(|| a.id.cmp(&b.id))
            }),
            Sort::Size => presets.sort_by(|a, b| {
                let area = |preset: &CanvasPreset| preset.width as u64 * preset.height as u64;
                area(b)
                    .cmp(&area(a))
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            }),
            Sort::RecentlyUsed => {
                let rank = |preset: &CanvasPreset| {
                    self.recent
                        .iter()
                        .position(|known| known.id == preset.id)
                        .unwrap_or(usize::MAX)
                };
                presets.sort_by(|a, b| {
                    rank(a)
                        .cmp(&rank(b))
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
            }
        }

        presets
    }

    /// Write the artist's custom presets to a shareable JSON file.
    pub fn export_to(&self, path: &Path) -> Result<usize, String> {
        if self.custom.is_empty() {
            return Err(String::from(
                "There are no custom presets to export yet. Save one first.",
            ));
        }
        let file = ExportFile {
            version: LIBRARY_VERSION,
            presets: self.custom.clone(),
        };
        let json = serde_json::to_vec_pretty(&file)
            .map_err(|error| format!("Could not serialise the presets: {error}"))?;
        fs::write(path, json)
            .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
        Ok(self.custom.len())
    }

    /// Read presets from a JSON file exported by [`Self::export_to`] and add
    /// them, renaming ids that would overwrite an existing preset.
    pub fn import_from(&mut self, path: &Path) -> Result<usize, String> {
        let bytes = fs::read(path)
            .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
        let file: ExportFile = serde_json::from_slice(&bytes).map_err(|error| {
            format!("{} is not an INKFORGE preset file: {error}", path.display())
        })?;
        if file.presets.is_empty() {
            return Err(String::from("That file contains no presets."));
        }

        let known: Vec<String> = self.all_ids();
        let mut imported = 0usize;
        for preset in file.presets {
            let mut preset = preset.sanitised();
            if preset.dpi <= 0.0 {
                continue;
            }
            if known.contains(&preset.id) || self.custom.iter().any(|own| own.id == preset.id) {
                preset.id = self.next_custom_id();
            }
            self.custom.push(preset);
            imported += 1;
        }
        Ok(imported)
    }
}

/// A short, unique suffix for generated preset ids.
fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn millimetres_convert_to_the_published_pixel_sizes() {
        assert_eq!(unit_to_px(210.0, Unit::Millimeters, 300.0), 2480);
        assert_eq!(unit_to_px(297.0, Unit::Millimeters, 300.0), 3508);
        assert_eq!(unit_to_px(8.5, Unit::Inches, 300.0), 2550);
        assert_eq!(unit_to_px(21.0, Unit::Centimeters, 300.0), 2480);
    }

    #[test]
    fn conversions_round_trip_within_a_pixel() {
        for unit in Unit::ALL {
            let px = 2480;
            let value = px_to_unit(px, unit, 300.0);
            let back = unit_to_px(value, unit, 300.0);
            assert!(
                back.abs_diff(px) <= 1,
                "{unit:?} round-tripped {px} to {back}"
            );
        }
    }

    #[test]
    fn bad_measurements_never_produce_a_zero_pixel_canvas() {
        assert_eq!(unit_to_px(0.0, Unit::Millimeters, 300.0), MIN_CANVAS_DIM);
        assert_eq!(
            unit_to_px(f32::NAN, Unit::Millimeters, 300.0),
            MIN_CANVAS_DIM
        );
        assert_eq!(unit_to_px(1.0e9, Unit::Inches, 300.0), MAX_CANVAS_DIM);
    }

    #[test]
    fn the_aspect_lock_keeps_the_ratio_while_editing() {
        let mut setup = CanvasSetup::default();
        setup.width = 3000;
        setup.height = 2000;
        setup.aspect_lock = true;
        setup.set_width_px(1500);
        assert_eq!(setup.height, 1000, "height follows width");

        setup.set_height_px(3000);
        assert_eq!(setup.width, 4500, "width follows height");
    }

    #[test]
    fn the_orientation_switch_swaps_the_page() {
        let mut setup = CanvasSetup::default();
        setup.width = 1080;
        setup.height = 1920;
        assert!(setup.is_portrait());
        setup.toggle_orientation();
        assert_eq!((setup.width, setup.height), (1920, 1080));
        assert!(!setup.is_portrait());
    }

    #[test]
    fn bleed_adds_pixels_on_both_sides_and_moves_the_trim_line() {
        let mut setup = CanvasSetup::default();
        setup.width = 2480;
        setup.height = 3508;
        setup.dpi = 300.0;
        setup.set_bleed_mm(3.0);

        // 3 mm at 300 dpi is 35.4 px.
        assert!((setup.bleed_px - 35.43).abs() < 0.01, "{}", setup.bleed_px);
        let (width, height) = setup.canvas_size_px();
        assert_eq!(width, 2480 + 70);
        assert_eq!(height, 3508 + 70);
        assert!((setup.bleed_mm() - 3.0).abs() < 0.01, "read back in mm");
    }

    #[test]
    fn validation_refuses_zero_resolution_and_absurd_sizes() {
        let mut setup = CanvasSetup::default();
        setup.dpi = 0.0;
        assert!(setup.validate().is_err());

        setup.dpi = 300.0;
        setup.width = 15;
        assert!(setup.validate().is_err(), "below the minimum edge");

        setup.width = 20_000;
        setup.height = 20_000;
        assert!(
            setup.validate().is_err(),
            "400 MP must be refused before allocating"
        );
    }

    #[test]
    fn the_catalog_has_every_documented_preset() {
        let presets = builtin_presets();
        assert_eq!(presets.len(), 53);

        let mut ids: Vec<&str> = presets.iter().map(|preset| preset.id.as_str()).collect();
        ids.sort_unstable();
        let unique = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), unique, "preset ids must be unique");

        for category in Category::ALL {
            if category == Category::Custom {
                continue;
            }
            assert!(
                presets.iter().any(|preset| preset.category == category),
                "{category:?} has no presets"
            );
        }
    }

    #[test]
    fn print_presets_match_the_paper_they_name() {
        let presets = builtin_presets();
        let a4 = presets
            .iter()
            .find(|preset| preset.id == "print.a4")
            .unwrap();
        assert_eq!((a4.width, a4.height), (2480, 3508));
        assert_eq!(
            (a4.print_size_mm().0.round(), a4.print_size_mm().1.round()),
            (210.0, 297.0)
        );

        let letter = presets
            .iter()
            .find(|preset| preset.id == "print.letter")
            .unwrap();
        assert_eq!((letter.width, letter.height), (2550, 3300));

        let poster = presets
            .iter()
            .find(|preset| preset.id == "print.poster")
            .unwrap();
        assert_eq!((poster.width, poster.height), (5400, 7200));
    }

    #[test]
    fn a_new_canvas_carries_its_print_setup_and_sketch_layer() {
        let mut setup = CanvasSetup::default();
        setup.name = String::from("Poster");
        setup.apply_preset(
            &builtin_presets()
                .into_iter()
                .find(|preset| preset.id == "print.a3")
                .unwrap(),
        );
        setup.set_bleed_mm(3.0);
        setup.set_safe_mm(10.0);
        setup.sketch_layer = true;
        setup.background = BackgroundSpec::Transparent;

        let canvas = setup.to_canvas().expect("canvas");
        assert_eq!(canvas.name(), "Poster");
        assert_eq!(canvas.dpi(), 300.0);
        assert!(canvas.bleed_px() > 35.0);
        assert!(canvas.safe_px() > 118.0);
        assert!(canvas.background().fill().is_none());
        assert_eq!(canvas.layer_count(), 2, "background layer plus the sketch");
        assert_eq!(canvas.active_entry().name(), "Sketch");
        assert!(!canvas.modified(), "a fresh document has nothing to save");
    }

    #[test]
    fn the_library_persists_custom_presets_favourites_and_recents() {
        let dir = std::env::temp_dir().join(format!(
            "inkforge-preset-lib-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        let _ = fs::remove_dir_all(&dir);

        let mut library = PresetLibrary::new();
        let mut preset =
            CanvasPreset::new("custom.one", "My page", Category::Custom, 1200, 1600, 300.0);
        preset.bleed_px = 20.0;
        library.upsert(preset);
        library.toggle_favorite("print.a4");
        library.remember_use(CanvasPreset::new(
            "custom.one",
            "My page",
            Category::Custom,
            1200,
            1600,
            300.0,
        ));
        library.save(&dir).expect("save");

        let loaded = PresetLibrary::load(&dir);
        assert_eq!(loaded.custom().len(), 1);
        assert_eq!(loaded.custom()[0].name, "My page");
        assert!(loaded.is_favorite("print.a4"));
        assert_eq!(loaded.recent().len(), 1);
        assert_eq!(loaded.all().len(), loaded.builtin_count() + 1);

        let list = loaded.browse(&Browse {
            scope: Scope::Favorites,
            ..Browse::default()
        });
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "print.a4");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_is_case_insensitive_and_filters_by_category() {
        let library = PresetLibrary::new();
        let found = library.browse(&Browse {
            query: String::from("WEBTOON"),
            ..Browse::default()
        });
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "comic.webtoon");

        let print = library.browse(&Browse {
            category: Some(Category::Print),
            ..Browse::default()
        });
        assert_eq!(print.len(), 11);
        assert!(print
            .iter()
            .all(|preset| preset.category == Category::Print));
    }

    #[test]
    fn restoring_defaults_keeps_the_builtins() {
        let mut library = PresetLibrary::new();
        library.upsert(CanvasPreset::new(
            "custom.x",
            "X",
            Category::Custom,
            64,
            64,
            72.0,
        ));
        library.toggle_favorite("print.a5");
        library.restore_defaults();

        assert!(library.custom().is_empty());
        assert!(library.favorites().is_empty());
        assert_eq!(library.all().len(), library.builtin_count());
    }

    #[test]
    fn a_damaged_library_still_offers_the_builtins() {
        let dir = std::env::temp_dir().join(format!(
            "inkforge-preset-broken-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        let _ = fs::create_dir_all(&dir);
        fs::write(dir.join(LIBRARY_FILE), b"{ not json").expect("write junk");

        let library = PresetLibrary::load(&dir);
        assert!(library.custom().is_empty());
        assert_eq!(library.all().len(), library.builtin_count());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn presets_export_and_import_without_colliding() {
        let dir = std::env::temp_dir().join(format!(
            "inkforge-preset-io-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("dir");

        let mut source = PresetLibrary::new();
        source.upsert(CanvasPreset::new(
            "custom.share",
            "Shared page",
            Category::Custom,
            1500,
            2500,
            300.0,
        ));
        let file = dir.join("presets.json");
        assert_eq!(source.export_to(&file).expect("export"), 1);

        let mut target = PresetLibrary::new();
        assert_eq!(target.import_from(&file).expect("import"), 1);
        assert_eq!(target.custom().len(), 1);
        assert_eq!(target.custom()[0].name, "Shared page");

        // Importing the same file again must not overwrite the first copy.
        assert_eq!(target.import_from(&file).expect("import again"), 1);
        assert_eq!(target.custom().len(), 2);
        assert_ne!(target.custom()[0].id, target.custom()[1].id);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ratio_labels_are_readable_for_odd_pixel_sizes() {
        assert_eq!(reduce_ratio(3000, 4000), (3, 4));
        assert_eq!(reduce_ratio(1920, 1080), (16, 9));
        assert_eq!(reduce_ratio(1080, 1350), (4, 5));
        let a4 = reduce_ratio(2480, 3508);
        assert!(a4.0 < 100 && a4.1 < 100, "A4 reduces to {a4:?}");
    }
}

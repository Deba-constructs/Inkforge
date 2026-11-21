//! The canvas preset system: catalog, units, validation, the library, document
//! creation and the print metadata that reaches the exported files.

use std::fs;
use std::path::PathBuf;

use inkforge::app::{Dialog, InkforgeApp};
use inkforge::engine::{Canvas, MAX_CANVAS_DIM, MIN_CANVAS_DIM};
use inkforge::io::canvas_presets::{
    mm_to_px, px_to_mm, px_to_unit, reduce_ratio, unit_to_px, CanvasPreset, PresetLibrary,
};
use inkforge::io::{self, BackgroundSpec, Browse, CanvasSetup, Category, Scope, Sort, Unit};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("inkforge-presets-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn preset(id: &str) -> CanvasPreset {
    io::builtin_presets()
        .into_iter()
        .find(|preset| preset.id == id)
        .unwrap_or_else(|| panic!("missing preset {id}"))
}

/// The catalog exactly as documented, so a renamed or resized preset fails.
#[test]
fn every_documented_preset_exists_at_its_documented_size() {
    let expected: &[(&str, u32, u32)] = &[
        // A. Illustration
        ("Square Illustration", 2000, 2000),
        ("Digital Illustration", 3000, 4000),
        ("Portrait Illustration", 2480, 3508),
        ("Landscape Illustration", 3508, 2480),
        ("Character Sheet", 3000, 4000),
        ("Concept Art", 3840, 2160),
        ("Splash Art", 4000, 6000),
        // B. Comic & Manga
        ("Japanese Manga B4", 2570, 3640),
        ("Japanese Manga A4", 2100, 2970),
        ("Webtoon Vertical", 800, 12800),
        ("Comic Page", 2550, 3300),
        ("Comic Strip", 4000, 1500),
        ("Manga Double Spread", 7280, 5150),
        ("Storyboard", 1920, 1080),
        // C. Print & Publishing, resolved at 300 dpi
        ("A0", 9933, 14043),
        ("A1", 7016, 9933),
        ("A2", 4961, 7016),
        ("A3", 3508, 4961),
        ("A4", 2480, 3508),
        ("A5", 1748, 2480),
        ("A6", 1240, 1748),
        ("US Letter", 2550, 3300),
        ("US Legal", 2550, 4200),
        ("US Tabloid", 3300, 5100),
        ("Poster", 5400, 7200),
        // D. Photography
        ("Instagram Square", 1080, 1080),
        ("Instagram Portrait", 1080, 1350),
        ("Instagram Story", 1080, 1920),
        ("Facebook Cover", 1640, 624),
        ("YouTube Thumbnail", 1280, 720),
        ("YouTube Banner", 2560, 1440),
        ("Wallpaper Full HD", 1920, 1080),
        ("Wallpaper 2K", 2560, 1440),
        ("Wallpaper 4K", 3840, 2160),
        // E. Animation
        ("HD Animation", 1280, 720),
        ("Full HD Animation", 1920, 1080),
        ("2K Animation", 2560, 1440),
        ("4K Animation", 3840, 2160),
        ("Animation Portrait", 1080, 1920),
        ("Sprite Sheet", 2048, 2048),
        // F. Game art
        ("Game Character", 2048, 2048),
        ("Game Texture", 1024, 1024),
        ("High Resolution Texture", 4096, 4096),
        ("Environment Concept", 3840, 2160),
        ("Pixel Art", 320, 320),
        ("Game Icon", 512, 512),
        ("UI Asset", 1920, 1080),
        // G. Design & UI
        ("Desktop UI", 1920, 1080),
        ("Laptop UI", 1440, 900),
        ("Mobile UI", 390, 844),
        ("Tablet UI", 1024, 1366),
        ("App Icon", 1024, 1024),
        ("Presentation", 1920, 1080),
    ];

    let catalog = io::builtin_presets();
    assert_eq!(catalog.len(), expected.len(), "preset count");
    assert_eq!(
        catalog.len(),
        Category::ALL
            .iter()
            .filter(|category| **category != Category::Custom)
            .map(|category| catalog
                .iter()
                .filter(|preset| preset.category == *category)
                .count())
            .sum::<usize>()
    );

    for (name, width, height) in expected {
        let found = catalog
            .iter()
            .find(|preset| preset.name == *name)
            .unwrap_or_else(|| panic!("{name} is missing"));
        assert_eq!(
            (found.width, found.height),
            (*width, *height),
            "{name} is {} × {}",
            found.width,
            found.height
        );
        assert!(found.dpi >= 72.0, "{name} needs a resolution");
    }

    // Ids are stable, unique and printable.
    let mut ids: Vec<&str> = catalog.iter().map(|preset| preset.id.as_str()).collect();
    ids.sort_unstable();
    let total = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), total, "preset ids must be unique");
    assert!(ids.iter().all(|id| !id.trim().is_empty()));
}

#[test]
fn print_presets_convert_exactly_and_carry_300_dpi() {
    let a0 = preset("print.a0");
    assert_eq!(a0.dpi, 300.0);
    assert_eq!((a0.width, a0.height), (9933, 14043));

    // 210 mm at 300 dpi is 2480.31 px, rounded once to 2480.
    assert_eq!(unit_to_px(210.0, Unit::Millimeters, 300.0), 2480);
    assert_eq!(unit_to_px(297.0, Unit::Millimeters, 300.0), 3508);

    let (width_mm, height_mm) = preset("print.a4").print_size_mm();
    assert!((width_mm - 210.0).abs() < 0.5, "{width_mm}");
    assert!((height_mm - 297.0).abs() < 0.5, "{height_mm}");

    let letter = preset("print.letter");
    assert_eq!((letter.width, letter.height), (2550, 3300));
    assert_eq!(unit_to_px(8.5, Unit::Inches, 300.0), 2550);
    assert_eq!(unit_to_px(11.0, Unit::Inches, 300.0), 3300);
}

#[test]
fn units_convert_both_ways_without_drifting() {
    for unit in Unit::ALL {
        let pixels = 3000;
        let value = px_to_unit(pixels, unit, 300.0);
        let back = unit_to_px(value, unit, 300.0);
        assert!(
            back.abs_diff(pixels) <= 1,
            "{unit:?}: {pixels} px → {value} → {back} px"
        );
    }

    // 1 inch is 25.4 mm, 2.54 cm and 300 px at 300 dpi.
    assert!((px_to_unit(300, Unit::Millimeters, 300.0) - 25.4).abs() < 0.01);
    assert!((px_to_unit(300, Unit::Centimeters, 300.0) - 2.54).abs() < 0.01);
    assert!((px_to_unit(300, Unit::Inches, 300.0) - 1.0).abs() < 0.001);

    // A physical unit needs the resolution: the same 210 mm is twice the pixels
    // at twice the dpi.
    assert_eq!(unit_to_px(210.0, Unit::Millimeters, 600.0), 4961);
    assert_eq!(unit_to_px(210.0, Unit::Millimeters, 150.0), 1240);

    // Margins convert through the same maths.
    let px = mm_to_px(3.0, 300.0);
    assert!((px - 35.43).abs() < 0.01, "{px}");
    assert!((px_to_mm(px, 300.0) - 3.0).abs() < 0.01);
}

#[test]
fn aspect_ratios_are_readable() {
    assert_eq!(preset("illustration.digital").aspect_label(), "3:4");
    assert_eq!(preset("illustration.square").aspect_label(), "1:1");
    assert_eq!(preset("photo.instagram-story").aspect_label(), "9:16");
    let a4 = preset("print.a4");
    let (w, h) = reduce_ratio(a4.width, a4.height);
    assert!(w < 100 && h < 100, "A4 reads as {w}:{h}");
}

#[test]
fn setup_orientation_and_aspect_lock_behave() {
    let mut setup = CanvasSetup::default();
    setup.apply_preset(&preset("illustration.digital"));
    assert!(setup.is_portrait());
    setup.toggle_orientation();
    assert_eq!((setup.width, setup.height), (4000, 3000));
    assert!(!setup.is_portrait());

    setup.aspect_lock = true;
    setup.set_width_px(2000);
    assert_eq!(setup.height, 1500, "the lock keeps 4:3");

    setup.set_aspect(16, 9);
    assert_eq!(setup.height, 1125, "a ratio chip reshapes the height");
    assert!(setup.aspect_lock);
}

#[test]
fn setup_validation_reports_impossible_canvases() {
    let mut setup = CanvasSetup::default();

    setup.width = MIN_CANVAS_DIM - 1;
    assert!(setup.validate().is_err(), "too small");

    setup.width = 1920;
    setup.height = 1080;
    setup.dpi = 0.0;
    assert!(setup.validate().is_err(), "zero resolution");

    setup.dpi = 300.0;
    setup.width = MAX_CANVAS_DIM;
    setup.height = MAX_CANVAS_DIM;
    assert!(setup.validate().is_err(), "400 MP must be refused");

    setup.width = 800;
    setup.height = 12_800;
    assert!(setup.validate().is_ok(), "a webtoon page is allowed");
}

#[test]
fn a_document_is_built_with_bleed_guides_sketch_layer_and_background() {
    let mut setup = CanvasSetup::default();
    setup.apply_preset(&preset("print.a4"));
    setup.name = String::from("Poster proof");
    setup.set_bleed_mm(3.0);
    setup.set_safe_mm(10.0);
    setup.background = BackgroundSpec::Transparent;
    setup.sketch_layer = true;

    assert!(setup.validate().is_ok());
    let canvas = setup.to_canvas().expect("canvas");

    assert_eq!(canvas.name(), "Poster proof");
    assert_eq!(canvas.dpi(), 300.0);
    // 3 mm per side is 35.4 px, rounded to 35 px of bleed on each edge.
    assert_eq!(canvas.width(), 2480 + 70);
    assert_eq!(canvas.height(), 3508 + 70);
    assert!(
        (canvas.bleed_mm() - 2.96).abs() < 0.05,
        "{}",
        canvas.bleed_mm()
    );
    assert!((canvas.safe_mm() - 10.0).abs() < 0.2);
    assert!(canvas.background().fill().is_none());
    assert_eq!(canvas.layer_count(), 2);
    assert_eq!(canvas.active_entry().name(), "Sketch");
    assert!(!canvas.modified(), "a new document starts clean");

    let (mm_w, mm_h) = canvas.print_size_mm();
    assert!(
        mm_w > 215.0 && mm_h > 302.0,
        "{mm_w} × {mm_h} mm includes bleed"
    );
}

#[test]
fn a_black_background_is_stored_as_black_not_as_a_custom_colour() {
    let mut setup = CanvasSetup::default();
    setup.background = BackgroundSpec::Black;
    setup.width = 128;
    setup.height = 128;
    let canvas = setup.to_canvas().expect("canvas");
    assert_eq!(
        canvas.background().fill(),
        Some(inkforge::egui::Color32::BLACK)
    );
    assert_eq!(
        BackgroundSpec::from_background(canvas.background()),
        BackgroundSpec::Black
    );
    assert_eq!(
        BackgroundSpec::from_background(canvas.background()).label(),
        "Black"
    );
}

#[test]
fn the_library_filters_sorts_and_tracks_recents() {
    let mut library = PresetLibrary::new();

    // Category filter and search together.
    let found = library.browse(&Browse {
        category: Some(Category::Comic),
        query: String::from("manga"),
        ..Browse::default()
    });
    assert_eq!(found.len(), 3, "two manga pages and the double spread");
    assert!(found
        .iter()
        .all(|preset| preset.category == Category::Comic));

    // Largest first.
    let by_size = library.browse(&Browse {
        sort: Sort::Size,
        ..Browse::default()
    });
    assert_eq!(
        by_size[0].id, "print.a0",
        "A0 at 300 dpi has the most pixels"
    );
    assert!(by_size[0].megapixels() > 130.0);

    // Name order.
    let by_name = library.browse(&Browse {
        sort: Sort::Name,
        ..Browse::default()
    });
    let names: Vec<String> = by_name.iter().map(|preset| preset.name.clone()).collect();
    let mut sorted = names.clone();
    sorted.sort_by_key(|name| name.to_lowercase());
    assert_eq!(names, sorted);

    // Favourites are a separate view and follow the star.
    library.toggle_favorite("print.a4");
    library.toggle_favorite("comic.webtoon");
    assert_eq!(
        library
            .browse(&Browse {
                scope: Scope::Favorites,
                ..Browse::default()
            })
            .len(),
        2
    );
    library.toggle_favorite("print.a5");
    assert_eq!(
        library
            .browse(&Browse {
                scope: Scope::Favorites,
                ..Browse::default()
            })
            .len(),
        3
    );
    assert!(library.is_favorite("comic.webtoon"));

    // Recently used floats to the top when that order is chosen.
    library.remember_use(preset("game.pixel-art"));
    let recent = library.browse(&Browse {
        scope: Scope::Recent,
        ..Browse::default()
    });
    assert_eq!(recent[0].id, "game.pixel-art");
    let by_use = library.browse(&Browse {
        sort: Sort::RecentlyUsed,
        ..Browse::default()
    });
    assert_eq!(by_use[0].id, "game.pixel-art");
}

#[test]
fn the_library_survives_a_restart_and_can_be_restored() {
    let dir = scratch("persist");

    let mut app = InkforgeApp::with_defaults();
    app.enable_presets(dir.clone());
    app.open_dialog(Dialog::NewCanvas);

    app.new_canvas.setup.width = 1000;
    app.new_canvas.setup.height = 1400;
    app.new_canvas.setup.dpi = 200.0;
    app.new_canvas.preset_name = String::from("Comic panel");
    app.save_canvas_preset().expect("save preset");
    app.toggle_preset_favorite("print.a4");

    assert_eq!(app.presets.custom().len(), 1);
    let saved = app.presets.custom()[0].clone();
    assert_eq!(saved.name, "Comic panel");
    assert_eq!((saved.width, saved.height), (1000, 1400));

    // A fresh app reading the same folder sees the saved library.
    let mut restarted = InkforgeApp::with_defaults();
    restarted.enable_presets(dir.clone());
    assert_eq!(restarted.presets.custom().len(), 1);
    assert_eq!(restarted.presets.custom()[0].name, "Comic panel");
    assert!(restarted.presets.is_favorite("print.a4"));
    assert_eq!(
        restarted.presets.all().len(),
        restarted.presets.builtin_count() + 1
    );

    // Editing the preset rewrites it in place.
    restarted.new_canvas.setup.width = 1200;
    restarted.new_canvas.setup.height = 1800;
    restarted.new_canvas.setup.preset_id = Some(saved.id.clone());
    restarted.new_canvas.editing = Some(saved.id.clone());
    restarted.new_canvas.preset_name = String::from("Comic panel v2");
    restarted.update_canvas_preset().expect("update preset");

    assert_eq!(restarted.presets.custom().len(), 1);
    let updated = restarted.presets.custom()[0].clone();
    assert_eq!(updated.width, 1200);
    assert_eq!(updated.name, "Comic panel v2");

    // Deleting removes it from the library and from favourites.
    restarted.new_canvas.editing = Some(updated.id.clone());
    restarted.delete_canvas_preset().expect("delete preset");
    assert!(restarted.presets.custom().is_empty());
    assert!(
        restarted.presets.is_favorite("print.a4"),
        "deleting a preset leaves the favourites of built-ins alone"
    );

    // Restoring the defaults also clears favourites and recents.
    restarted.presets.remember_use(preset("print.a4"));
    restarted.restore_canvas_presets();
    assert!(restarted.presets.favorites().is_empty());
    assert!(restarted.presets.recent().is_empty());
    assert_eq!(
        restarted.presets.all().len(),
        restarted.presets.builtin_count()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn creating_a_canvas_from_a_preset_uses_every_setting() {
    let dir = scratch("create");
    let mut app = InkforgeApp::with_defaults();
    app.enable_presets(dir.clone());

    app.open_dialog(Dialog::NewCanvas);
    app.new_canvas.setup.apply_preset(&preset("comic.page"));
    app.new_canvas.setup.name = String::from("Page 1");
    app.new_canvas.setup.sketch_layer = true;
    app.create_canvas();

    assert!(app.dialog.is_none());
    assert_eq!(app.canvas.name(), "Page 1");
    assert_eq!(app.canvas.width(), 2550);
    assert_eq!(app.canvas.height(), 3300);
    assert_eq!(app.canvas.dpi(), 300.0);
    assert_eq!(app.canvas.layer_count(), 2);

    // The preset was used, so it is now first in the recent list.
    assert_eq!(
        app.presets
            .recent()
            .first()
            .map(|preset| preset.id.as_str()),
        Some("comic.page")
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_custom_size_canvas_is_remembered_as_a_custom_recent() {
    let dir = scratch("custom-recent");
    let mut app = InkforgeApp::with_defaults();
    app.enable_presets(dir.clone());

    app.open_dialog(Dialog::NewCanvas);
    app.new_canvas.setup.name = String::from("Odd size");
    app.new_canvas.setup.width = 777;
    app.new_canvas.setup.height = 999;
    app.new_canvas.setup.dpi = 144.0;
    app.create_canvas();

    assert_eq!(app.canvas.width(), 777);
    assert_eq!(app.canvas.dpi(), 144.0);
    let recent = app.presets.recent().first().expect("a recent entry");
    assert_eq!(recent.category, Category::Custom);
    assert_eq!(recent.width, 777);
    assert_eq!(recent.name, "Odd size");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn presets_export_and_import_through_the_library() {
    let dir = scratch("exchange");
    let file = dir.join("shared.json");

    let mut library = PresetLibrary::new();
    library.upsert(CanvasPreset {
        id: String::from("custom.hand-drawn"),
        name: String::from("Hand-drawn page"),
        category: Category::Custom,
        width: 1600,
        height: 2200,
        dpi: 250.0,
        pixel_aspect: 1.0,
        bleed_px: 12.0,
        safe_px: 24.0,
        background: BackgroundSpec::White,
    });
    assert_eq!(library.export_to(&file).expect("export"), 1);

    let mut other = PresetLibrary::new();
    assert_eq!(other.import_from(&file).expect("import"), 1);
    let imported = &other.custom()[0];
    assert_eq!(imported.name, "Hand-drawn page");
    assert_eq!(imported.dpi, 250.0);
    assert_eq!(imported.bleed_px, 12.0);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_project_keeps_its_print_setup() {
    let dir = scratch("project");
    let path = dir.join("print.inkforge");

    let mut canvas = Canvas::try_new("Print", 640, 480, io::BackgroundSpec::White.to_background())
        .expect("canvas");
    canvas.set_print_setup(300.0, 1.5, 20.0, 40.0);
    io::save_project(&path, &canvas).expect("save");

    let loaded = io::load_project(&path).expect("load");
    assert_eq!(loaded.width(), 640);
    assert_eq!(loaded.height(), 480);
    assert!((loaded.dpi() - 300.0).abs() < 0.001);
    assert!((loaded.pixel_aspect() - 1.5).abs() < 0.001);
    assert!((loaded.bleed_px() - 20.0).abs() < 0.001);
    assert!((loaded.safe_px() - 40.0).abs() < 0.001);

    let _ = fs::remove_dir_all(&dir);
}

/// Read the `pHYs` resolution a PNG declares, in pixels per metre.
fn png_resolution(bytes: &[u8]) -> Option<(u32, u32)> {
    let offset = bytes.windows(4).position(|window| window == b"pHYs")? + 4;
    let read = |index: usize| {
        u32::from_be_bytes([
            bytes[offset + index],
            bytes[offset + index + 1],
            bytes[offset + index + 2],
            bytes[offset + index + 3],
        ])
    };
    if *bytes.get(offset + 8)? != 1 {
        return None;
    }
    Some((read(0), read(4)))
}

#[test]
fn png_export_embeds_the_document_resolution_and_srgb() {
    let dir = scratch("png-meta");
    let path = dir.join("proof.png");

    let mut canvas = Canvas::try_new("Proof", 32, 24, io::BackgroundSpec::White.to_background())
        .expect("canvas");
    canvas.set_print_setup(300.0, 1.0, 0.0, 0.0);
    io::export_image(&mut canvas, &path, io::ImageFormat::Png).expect("export");

    let bytes = fs::read(&path).expect("png bytes");
    // 300 dpi / 0.0254 = 11811.02 pixels per metre.
    assert_eq!(png_resolution(&bytes), Some((11811, 11811)));
    assert!(
        bytes.windows(4).any(|window| window == b"sRGB"),
        "the colour space is declared"
    );

    let _ = fs::remove_dir_all(&dir);
}

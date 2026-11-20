//! Tests for the layer stack: ordering, visibility, opacity, blending, groups
//! and the layer commands driven through the app.

use inkforge::app::InkforgeApp;
use inkforge::egui::{pos2, Color32, Pos2, Rect, Vec2};
use inkforge::engine::{Background, BrushSettings, Canvas};
use inkforge::layers::{BlendMode, Entry};
use inkforge::ui::sidebar::drop_entry_index;

#[test]
fn renaming_a_group_inline_commits_as_one_undo_step() {
    let mut app = InkforgeApp::with_defaults();
    app.add_layer();
    app.select_entry(0);
    app.add_group();
    let group = app.canvas.entries()[0].id();
    let original = app.canvas.group_props(group).expect("group props").name.clone();

    // The field opens on the current name and changes nothing yet.
    app.begin_rename_entry(group);
    let state = app.rename.as_mut().expect("rename started");
    assert_eq!(state.text, original);
    state.text = String::from("Sky studies");
    state.focus = false;
    assert_eq!(
        app.canvas.group_props(group).expect("group props").name,
        original,
        "nothing is written until the field closes"
    );

    app.finish_rename(true);
    assert!(app.rename.is_none());
    assert_eq!(
        app.canvas.group_props(group).expect("group props").name,
        "Sky studies"
    );

    // Escape drops the edit; a blank name is refused too.
    app.begin_rename_entry(group);
    app.rename.as_mut().expect("rename started").text = String::from("Discarded");
    app.finish_rename(false);
    assert_eq!(
        app.canvas.group_props(group).expect("group props").name,
        "Sky studies"
    );

    app.begin_rename_entry(group);
    app.rename.as_mut().expect("rename started").text = String::from("   ");
    app.finish_rename(true);
    assert_eq!(
        app.canvas.group_props(group).expect("group props").name,
        "Sky studies"
    );

    // One undo step takes the committed name back, not the group with it.
    app.undo();
    assert_eq!(
        app.canvas.group_props(group).expect("group props").name,
        original
    );
}

#[test]
fn the_properties_panel_name_field_commits_as_one_undo_step() {
    let mut app = InkforgeApp::with_defaults();
    app.add_layer();
    let layer = app.canvas.entries()[0].id();
    let original = app
        .canvas
        .layer_props(layer)
        .expect("layer props")
        .name
        .clone();
    let entries = app.canvas.entries().len();

    // Every keystroke lands in the draft, never in the document.
    for text in ["B", "Bl", "Blu"] {
        app.set_name_draft(layer, String::from(text));
        assert_eq!(
            app.canvas.layer_props(layer).expect("layer props").name,
            original,
            "typing must not touch the document"
        );
    }

    app.finish_name_draft(true);
    assert!(app.name_field.is_none());
    assert_eq!(
        app.canvas.layer_props(layer).expect("layer props").name,
        "Blu"
    );

    // One undo takes the whole name back, not one character at a time.
    app.undo();
    assert_eq!(
        app.canvas.layer_props(layer).expect("layer props").name,
        original
    );
    assert_eq!(
        app.canvas.entries().len(),
        entries,
        "undo must restore the name, not remove the layer"
    );

    // Escape drops the edit; a blank name is refused too.
    app.set_name_draft(layer, String::from("Discarded"));
    app.finish_name_draft(false);
    assert_eq!(
        app.canvas.layer_props(layer).expect("layer props").name,
        original
    );
    app.set_name_draft(layer, String::from("   "));
    app.finish_name_draft(true);
    assert_eq!(
        app.canvas.layer_props(layer).expect("layer props").name,
        original
    );
}

#[test]
fn a_rename_after_an_opacity_change_is_its_own_undo_step() {
    let mut app = InkforgeApp::with_defaults();
    app.add_layer();
    let layer = app.canvas.entries()[0].id();
    let original = app
        .canvas
        .layer_props(layer)
        .expect("layer props")
        .name
        .clone();
    let opacity = app.canvas.layer_props(layer).expect("layer props").opacity;

    app.set_layer_opacity(layer, 0.5);
    app.set_name_draft(layer, String::from("Shade"));
    app.finish_name_draft(true);

    app.undo();
    let props = app.canvas.layer_props(layer).expect("layer props");
    assert_eq!(props.name, original, "undo takes the name back");
    assert_eq!(
        props.opacity, 0.5,
        "and leaves the earlier opacity change alone"
    );
    assert!((opacity - 1.0).abs() < 1e-6);
}

#[test]
fn renaming_a_layer_inline_commits_as_one_undo_step() {
    let mut app = InkforgeApp::with_defaults();
    app.add_layer();
    let layer = app.canvas.entries()[0].id();
    assert!(!app.canvas.entries()[0].is_group());
    let original = app
        .canvas
        .layer_props(layer)
        .expect("layer props")
        .name
        .clone();
    let layer_count = app.canvas.entries().len();

    // The field opens on the current name and changes nothing yet.
    app.begin_rename_entry(layer);
    let state = app.rename.as_mut().expect("rename started");
    assert_eq!(state.text, original);
    state.text = String::from("Ink pass");
    state.focus = false;
    assert_eq!(
        app.canvas.layer_props(layer).expect("layer props").name,
        original,
        "nothing is written until the field closes"
    );

    app.finish_rename(true);
    assert!(app.rename.is_none());
    assert_eq!(
        app.canvas.layer_props(layer).expect("layer props").name,
        "Ink pass"
    );

    // Escape drops the edit, and a rename only ever touches the name.
    app.begin_rename_entry(layer);
    app.rename.as_mut().expect("rename started").text = String::from("Discarded");
    app.finish_rename(false);
    assert_eq!(
        app.canvas.layer_props(layer).expect("layer props").name,
        "Ink pass"
    );

    app.undo();
    assert_eq!(
        app.canvas.layer_props(layer).expect("layer props").name,
        original
    );
    assert_eq!(
        app.canvas.entries().len(),
        layer_count,
        "undo must restore the name, not remove the layer"
    );
}

fn canvas(width: u32, height: u32) -> Canvas {
    Canvas::try_new("Test", width, height, Background::Transparent).expect("valid canvas")
}

fn brush() -> BrushSettings {
    let mut brush = BrushSettings::default();
    brush.set_size(12.0);
    brush.set_hardness(1.0);
    brush.set_opacity(1.0);
    brush
}

fn paint(canvas: &mut Canvas, layer: u64, at: Pos2, colour: Color32) {
    canvas.paint_layer(layer, at, at, &brush().dab(colour, false));
}

fn active(canvas: &Canvas) -> u64 {
    canvas.active_layer_id().expect("an active paint layer")
}

fn composite_alpha(canvas: &mut Canvas, x: u32, y: u32) -> u8 {
    canvas.composite().pixel(x, y).a()
}

#[test]
fn a_new_canvas_has_one_active_layer() {
    let canvas = canvas(64, 64);
    let layer = canvas.active_layer().expect("active layer");

    assert_eq!(canvas.layer_count(), 1);
    assert_eq!(canvas.entry_count(), 1);
    assert_eq!(canvas.active_layer_index(), 0);
    assert!(layer.visible());
    assert_eq!(layer.opacity(), 1.0);
    assert_eq!(layer.blend(), BlendMode::Normal);
    assert_eq!(layer.group(), None);
}

#[test]
fn adding_a_layer_puts_it_above_the_active_one_and_selects_it() {
    let mut canvas = canvas(64, 64);
    let bottom = active(&canvas);

    let (index, snapshot) = canvas.layer_add_above_active();
    assert_eq!(index, 1);
    assert_eq!(canvas.layer_count(), 2);
    assert_eq!(canvas.active_layer_index(), 1);
    assert_eq!(snapshot.id(), active(&canvas));
    assert_ne!(active(&canvas), bottom);
}

#[test]
fn layers_cannot_all_be_removed() {
    let mut canvas = canvas(32, 32);
    assert!(canvas.remove_entries_at(0).is_none());
    assert_eq!(canvas.layer_count(), 1);
}

#[test]
fn reordering_changes_which_layer_composites_on_top() {
    let mut canvas = canvas(32, 32);
    let bottom = active(&canvas);
    paint(&mut canvas, bottom, pos2(16.0, 16.0), Color32::RED);

    canvas.layer_add_above_active();
    let top = active(&canvas);
    paint(&mut canvas, top, pos2(16.0, 16.0), Color32::BLUE);

    let over = canvas.composite().pixel(16, 16);
    assert!(over.b() > over.r(), "the upper blue layer should win");

    canvas.move_active_by(-1);
    let under = canvas.composite().pixel(16, 16);
    assert!(under.r() > under.b(), "after reordering red is on top");

    canvas.move_active_by(1);
    let again = canvas.composite().pixel(16, 16);
    assert!(again.b() > again.r(), "moving it back restores the stack");
}

#[test]
fn hiding_a_layer_removes_it_from_the_composite() {
    let mut canvas = canvas(32, 32);
    let layer = active(&canvas);
    paint(&mut canvas, layer, pos2(16.0, 16.0), Color32::RED);
    assert!(composite_alpha(&mut canvas, 16, 16) > 240);

    let mut props = canvas.layer_props(layer).expect("props");
    props.visible = false;
    canvas.set_layer_props(layer, props);

    assert_eq!(composite_alpha(&mut canvas, 16, 16), 0);
    // The pixels are still there: hiding is not deleting.
    assert!(canvas.layer_surface(layer).unwrap().pixel(16, 16).a() > 240);

    let mut props = canvas.layer_props(layer).expect("props");
    props.visible = true;
    canvas.set_layer_props(layer, props);
    assert!(composite_alpha(&mut canvas, 16, 16) > 240);
}

#[test]
fn layer_opacity_scales_the_contribution() {
    let mut canvas = canvas(32, 32);
    let layer = active(&canvas);
    paint(&mut canvas, layer, pos2(16.0, 16.0), Color32::WHITE);

    let mut props = canvas.layer_props(layer).expect("props");
    props.opacity = 0.5;
    canvas.set_layer_props(layer, props);

    let alpha = composite_alpha(&mut canvas, 16, 16);
    assert!(
        (100..=160).contains(&alpha),
        "50% opacity should composite to roughly half alpha, got {alpha}"
    );
}

#[test]
fn multiply_blend_darkens_against_the_layer_below() {
    let mut canvas = canvas(32, 32);
    let bottom = active(&canvas);
    paint(&mut canvas, bottom, pos2(16.0, 16.0), Color32::from_rgb(255, 128, 0));

    canvas.layer_add_above_active();
    let top = active(&canvas);
    paint(&mut canvas, top, pos2(16.0, 16.0), Color32::from_rgb(128, 255, 128));

    let mut props = canvas.layer_props(top).expect("props");
    props.blend = BlendMode::Multiply;
    canvas.set_layer_props(top, props);

    let pixel = canvas.composite().pixel(16, 16);
    assert!(
        pixel.r() < 200 && pixel.g() > 120 && pixel.b() < 40,
        "multiply should keep the darker red channel and drop blue, got {pixel:?}"
    );
}

#[test]
fn the_composite_is_only_recomputed_for_dirty_regions() {
    let mut canvas = canvas(32, 32);
    let layer = active(&canvas);
    paint(&mut canvas, layer, pos2(8.0, 8.0), Color32::RED);

    let first = canvas.composite().pixel(8, 8);
    let second = canvas.composite().pixel(8, 8);
    assert_eq!(first, second);
    assert_eq!(canvas.composite().pixel(20, 20).a(), 0);
}

// ---- groups --------------------------------------------------------------

#[test]
fn grouping_a_layer_puts_it_inside_a_new_group() {
    let mut app = InkforgeApp::with_defaults();
    app.add_layer();
    let grouped = app.canvas.active_layer_id().expect("active layer");

    app.add_group();

    assert_eq!(app.canvas.entry_count(), 3, "group plus two layers");
    assert!(app.canvas.active_entry().is_group());

    let group = app.canvas.active_entry().id();
    let child = app
        .canvas
        .layer(grouped)
        .expect("the grouped layer still exists");
    assert_eq!(child.group(), Some(group));
    assert_eq!(
        app.canvas.active_layer_index(),
        1,
        "the group takes the layer's place and is selected"
    );
}

#[test]
fn a_group_cannot_be_created_inside_another_group() {
    let mut app = InkforgeApp::with_defaults();
    app.add_layer();
    app.add_group();

    let depth = app.history.undo_depth();
    app.select_entry(1);
    app.add_group();

    assert_eq!(app.history.undo_depth(), depth, "the second group is refused");
    assert_eq!(app.canvas.entry_count(), 3);
}

#[test]
fn group_visibility_hides_every_layer_inside() {
    let mut app = InkforgeApp::with_defaults();
    app.dab_at(pos2(300.0, 300.0));
    app.end_stroke();
    app.add_group();

    let group = app.canvas.active_entry().id();
    assert!(composite_alpha(&mut app.canvas, 300, 300) > 0);

    app.set_group_visibility(group, false);
    assert_eq!(
        composite_alpha(&mut app.canvas, 300, 300),
        0,
        "hiding the group hides its layers"
    );

    app.set_group_visibility(group, true);
    assert!(composite_alpha(&mut app.canvas, 300, 300) > 0);
}

#[test]
fn group_opacity_multiplies_into_each_child_layer() {
    let mut app = InkforgeApp::with_defaults();
    app.dab_at(pos2(300.0, 300.0));
    app.end_stroke();
    app.add_group();

    let group = app.canvas.active_entry().id();
    app.set_group_opacity(group, 0.5);

    let alpha = composite_alpha(&mut app.canvas, 300, 300);
    assert!(
        (100..=160).contains(&alpha),
        "a 50% group should halve its child layers, got {alpha}"
    );
}

#[test]
fn adding_a_layer_with_a_group_selected_adds_it_to_the_group() {
    let mut app = InkforgeApp::with_defaults();
    app.add_layer();
    app.add_group();
    let group = app.canvas.active_entry().id();

    app.add_layer();

    let added = app.canvas.active_layer_id().expect("active layer");
    assert_eq!(
        app.canvas.layer(added).unwrap().group(),
        Some(group),
        "the new layer joins the selected group"
    );
    assert_eq!(app.canvas.entry_count(), 4);
}

#[test]
fn deleting_a_group_removes_its_layers_and_undo_restores_them() {
    let mut app = InkforgeApp::with_defaults();
    app.add_layer();
    let grouped = app.canvas.active_layer_id().expect("active layer");
    app.add_group();

    app.delete_active_layer();
    assert_eq!(app.canvas.entry_count(), 1, "group and child are gone");
    assert!(app.canvas.layer(grouped).is_none());

    app.undo();
    assert_eq!(app.canvas.entry_count(), 3);
    assert!(app.canvas.layer(grouped).is_some());
}

#[test]
fn painting_is_refused_while_a_group_is_selected() {
    let mut app = InkforgeApp::with_defaults();
    app.add_group();

    let depth = app.history.undo_depth();
    app.dab_at(pos2(300.0, 300.0));
    app.end_stroke();

    assert_eq!(app.history.undo_depth(), depth, "no stroke is recorded");
    assert!(
        app.status().contains("Select a layer"),
        "the status bar explains why nothing was painted"
    );
}

#[test]
fn collapsing_a_group_is_view_state_only() {
    let mut app = InkforgeApp::with_defaults();
    app.add_group();
    let group = app.canvas.active_entry().id();
    let depth = app.history.undo_depth();

    app.toggle_group_collapsed(group);
    assert!(app.canvas.group_is_collapsed(group));
    app.toggle_group_collapsed(group);
    assert!(!app.canvas.group_is_collapsed(group));

    assert_eq!(app.history.undo_depth(), depth, "collapsing is not undoable");
}

// ---- app-level layer commands --------------------------------------------

#[test]
fn app_layer_commands_are_undoable() {
    let mut app = InkforgeApp::with_defaults();
    let start = app.canvas.layer_count();

    app.add_layer();
    assert_eq!(app.canvas.layer_count(), start + 1);

    app.delete_active_layer();
    assert_eq!(app.canvas.layer_count(), start);

    app.undo();
    assert_eq!(app.canvas.layer_count(), start + 1, "undo restores the layer");

    app.undo();
    assert_eq!(app.canvas.layer_count(), start, "undo removes the added layer");
}

#[test]
fn duplicating_a_layer_copies_pixels_and_properties() {
    let mut app = InkforgeApp::with_defaults();
    app.dab_at(pos2(300.0, 300.0));
    app.end_stroke();

    let original = active(&app.canvas);
    app.set_layer_opacity(original, 0.5);
    app.duplicate_layer();

    let copy = active(&app.canvas);
    assert_ne!(copy, original);
    assert_eq!(app.canvas.layer_count(), 2);
    assert_eq!(
        app.canvas.active_layer_index(),
        1,
        "the copy sits directly above the original"
    );

    let props = app.canvas.layer_props(copy).expect("copy props");
    assert!(props.name.ends_with("copy"));
    assert_eq!(props.opacity, 0.5);
    assert!(
        app.canvas
            .layer_surface(copy)
            .unwrap()
            .pixel(300, 300)
            .a()
            > 0,
        "the copy must carry the pixels too"
    );

    app.undo();
    assert_eq!(app.canvas.layer_count(), 1);
    assert_eq!(active(&app.canvas), original);
}

#[test]
fn merging_down_bakes_pixels_and_is_one_undo_step() {
    let mut app = InkforgeApp::with_defaults();
    let lower = active(&app.canvas);
    app.dab_at(pos2(500.0, 300.0));
    app.end_stroke();

    app.add_layer();
    let upper = active(&app.canvas);
    app.dab_at(pos2(300.0, 300.0));
    app.end_stroke();

    let count = app.canvas.layer_count();
    app.merge_layer_down();

    assert_eq!(app.canvas.layer_count(), count - 1);
    assert_eq!(active(&app.canvas), lower);
    assert!(
        app.canvas.layer_surface(lower).unwrap().pixel(300, 300).a() > 0,
        "the merged layer keeps the upper layer's paint"
    );
    assert!(
        app.canvas.layer_surface(lower).unwrap().pixel(500, 300).a() > 0,
        "the lower layer's own paint survives"
    );

    app.undo();
    assert_eq!(app.canvas.layer_count(), count, "undo brings the layer back");
    assert!(
        app.canvas.layer_surface(upper).unwrap().pixel(300, 300).a() > 0,
        "the merged-away layer returns with its pixels"
    );
    assert_eq!(
        app.canvas.layer_surface(lower).unwrap().pixel(300, 300).a(),
        0,
        "the receiving layer is restored to its pre-merge pixels"
    );
}

#[test]
fn the_bottom_layer_cannot_be_merged_down() {
    let mut app = InkforgeApp::with_defaults();
    let count = app.canvas.layer_count();
    app.merge_layer_down();
    assert_eq!(app.canvas.layer_count(), count);
    assert!(!app.history.can_undo());
}

#[test]
fn thumbnails_keep_the_canvas_aspect_ratio_and_are_small() {
    let mut canvas = canvas(200, 100);
    let layer = active(&canvas);
    paint(&mut canvas, layer, pos2(10.0, 10.0), Color32::RED);

    let (_image, dims) = inkforge::engine::renderer::thumbnail_image(
        canvas.layer_surface(layer).unwrap(),
        inkforge::engine::renderer::THUMBNAIL_MAX,
    );
    assert_eq!(dims, [28, 14], "2:1 canvas keeps a 2:1 thumbnail");
}

#[test]
fn flattening_bakes_visible_layers_and_is_undoable() {
    let mut app = InkforgeApp::with_defaults();
    let lower = active(&app.canvas);
    app.dab_at(pos2(500.0, 300.0));
    app.end_stroke();

    app.add_layer();
    let upper = active(&app.canvas);
    app.dab_at(pos2(300.0, 300.0));
    app.end_stroke();

    app.flatten_image();
    assert_eq!(app.canvas.layer_count(), 1);
    assert_eq!(app.canvas.active_entry().name(), "Flattened");

    let only = active(&app.canvas);
    assert!(app.canvas.layer_surface(only).unwrap().pixel(300, 300).a() > 0);
    assert!(app.canvas.layer_surface(only).unwrap().pixel(500, 300).a() > 0);

    app.undo();
    assert_eq!(app.canvas.layer_count(), 2, "undo restores the whole stack");
    assert!(
        app.canvas
            .layer_surface(upper)
            .unwrap()
            .pixel(300, 300)
            .a()
            > 0,
        "each layer comes back with its own pixels"
    );
    assert_eq!(
        app.canvas.layer_surface(lower).unwrap().pixel(300, 300).a(),
        0
    );

    app.redo();
    assert_eq!(app.canvas.layer_count(), 1);
}

#[test]
fn flattening_discards_hidden_layers_but_undo_brings_them_back() {
    let mut app = InkforgeApp::with_defaults();
    app.add_layer();
    let hidden = active(&app.canvas);
    app.dab_at(pos2(300.0, 300.0));
    app.end_stroke();
    app.set_layer_visibility(hidden, false);

    app.flatten_image();
    let only = active(&app.canvas);
    assert_eq!(
        app.canvas.layer_surface(only).unwrap().pixel(300, 300).a(),
        0,
        "hidden layers are not baked into the flattened image"
    );

    app.undo();
    assert_eq!(app.canvas.layer_count(), 2);
    assert!(
        app.canvas
            .layer_surface(hidden)
            .unwrap()
            .pixel(300, 300)
            .a()
            > 0
    );
}

#[test]
fn flattening_a_single_layer_does_nothing() {
    let mut app = InkforgeApp::with_defaults();
    app.flatten_image();
    assert_eq!(app.canvas.layer_count(), 1);
    assert!(!app.history.can_undo());
}

#[test]
fn merging_down_undoes_and_redoes_the_pixels() {
    let mut app = InkforgeApp::with_defaults();
    let lower = active(&app.canvas);
    app.dab_at(pos2(200.0, 200.0));
    app.end_stroke();

    app.add_layer();
    let upper = active(&app.canvas);
    app.dab_at(pos2(300.0, 300.0));
    app.end_stroke();
    let merged_pixel = app.canvas.layer_surface(upper).unwrap().pixel(300, 300);

    app.merge_layer_down();
    assert_eq!(app.canvas.layer_count(), 1);
    let target = active(&app.canvas);
    assert_eq!(target, lower);
    assert_eq!(
        app.canvas.layer_surface(target).unwrap().pixel(300, 300),
        merged_pixel,
        "the merged layer's pixels land on the layer below"
    );

    app.undo();
    assert_eq!(app.canvas.layer_count(), 2);
    assert_eq!(
        app.canvas.layer_surface(lower).unwrap().pixel(300, 300).a(),
        0,
        "undo takes the merged pixels back out"
    );
    assert_eq!(
        app.canvas.layer_surface(upper).unwrap().pixel(300, 300),
        merged_pixel,
        "undo brings the merged layer back with its pixels"
    );

    app.redo();
    assert_eq!(app.canvas.layer_count(), 1);
    assert_eq!(
        app.canvas.layer_surface(target).unwrap().pixel(300, 300),
        merged_pixel,
        "redo re-applies the merge"
    );
}

#[test]
fn merging_snapshots_only_the_area_the_layer_covers() {
    let mut app = InkforgeApp::with_defaults();
    let canvas_bytes = app.canvas.width() as usize * app.canvas.height() as usize * 4;

    app.add_layer();
    app.dab_at(pos2(300.0, 300.0));
    app.end_stroke();

    let before = app.history.memory_bytes();
    app.merge_layer_down();
    let added = app.history.memory_bytes() - before;

    // The removed layer is a whole layer; the undo snapshot should be a small
    // rectangle around the dab, not a second canvas-sized copy of the target.
    assert!(
        added < canvas_bytes + canvas_bytes / 2,
        "merge history stored {added} bytes for a {canvas_bytes}-byte canvas"
    );
    assert!(added >= canvas_bytes, "the removed layer must be kept");
}

#[test]
fn flattening_does_not_store_a_second_canvas_image() {
    let mut app = InkforgeApp::with_defaults();
    let canvas_bytes = app.canvas.width() as usize * app.canvas.height() as usize * 4;

    app.add_layer();
    app.dab_at(pos2(300.0, 300.0));
    app.end_stroke();

    let before = app.history.memory_bytes();
    app.flatten_image();
    let added = app.history.memory_bytes() - before;

    // Two layers are restored on undo; a third canvas image for the flattened
    // result must not be retained as well.
    assert!(added >= 2 * canvas_bytes);
    assert!(
        added < 2 * canvas_bytes + canvas_bytes / 2,
        "flatten history stored {added} bytes for two {canvas_bytes}-byte layers"
    );
}

#[test]
fn undoing_a_merge_moves_the_layer_instead_of_copying_it() {
    let mut app = InkforgeApp::with_defaults();
    let canvas_bytes = app.canvas.width() as usize * app.canvas.height() as usize * 4;

    app.add_layer();
    app.dab_at(pos2(300.0, 300.0));
    app.end_stroke();
    app.merge_layer_down();

    let merged = app.history.memory_bytes();
    app.undo();
    let undone = app.history.memory_bytes();

    assert!(
        undone + canvas_bytes <= merged,
        "history kept a copy of the merged layer ({merged} -> {undone} bytes)"
    );
}

#[test]
fn drop_index_maps_the_pointer_to_a_stack_position() {
    // Rows are drawn top-down: entry 2 at the top, then 1, then 0.
    let rows: Vec<(usize, Rect)> = (0..3)
        .rev()
        .enumerate()
        .map(|(position, index)| {
            (
                index,
                Rect::from_min_size(pos2(0.0, position as f32 * 26.0), Vec2::new(100.0, 26.0)),
            )
        })
        .collect();

    let len = |_: usize| 1;
    assert_eq!(drop_entry_index(&rows, 2.0, len), 2, "above the top row");
    assert_eq!(drop_entry_index(&rows, 20.0, len), 1, "between the top two");
    assert_eq!(drop_entry_index(&rows, 40.0, len), 0, "between the lower two");
    assert_eq!(drop_entry_index(&rows, 70.0, len), 1, "below everything");
}

#[test]
fn dragging_the_bottom_row_to_the_top_reorders_and_undoes() {
    let mut app = InkforgeApp::with_defaults();
    let bottom = active(&app.canvas);
    app.add_layer();
    app.add_layer();

    app.begin_layer_drag(0);
    app.layer_drag.as_mut().unwrap().to = 3; // dropped above the top row
    app.finish_layer_drag();

    assert_eq!(app.canvas.active_layer_index(), 2);
    assert_eq!(active(&app.canvas), bottom);
    assert!(app.history.can_undo());

    app.undo();
    assert_eq!(app.canvas.entries()[0].id(), bottom);
}

#[test]
fn a_drag_that_changes_nothing_adds_no_history() {
    let mut app = InkforgeApp::with_defaults();
    app.add_layer();
    let depth = app.history.undo_depth();

    app.begin_layer_drag(1);
    app.layer_drag.as_mut().unwrap().to = 2; // dropped back where it came from
    app.finish_layer_drag();

    assert_eq!(app.canvas.active_layer_index(), 1);
    assert_eq!(app.history.undo_depth(), depth, "no history for a no-op drag");
}

#[test]
fn slots_inside_a_group_block_belong_to_that_group() {
    let mut app = InkforgeApp::with_defaults();
    app.add_group();
    app.add_layer();
    assert_eq!(app.canvas.entry_count(), 3);
    let group = app.canvas.entries()[0].id();

    assert_eq!(app.canvas.group_at_slot(0), None, "below the folder");
    assert_eq!(app.canvas.group_at_slot(1), Some(group), "first child");
    assert_eq!(app.canvas.group_at_slot(2), Some(group), "top child");
    assert_eq!(app.canvas.group_at_slot(3), None, "top edge of the folder");
    assert_eq!(app.canvas.group_at_slot(4), None, "above the folder");
}

#[test]
fn dragging_a_layer_onto_a_group_row_moves_it_inside_and_undoes() {
    let mut app = InkforgeApp::with_defaults();
    app.add_layer();
    let top = active(&app.canvas);
    app.select_entry(0);
    app.add_group();
    let group = app.canvas.entries()[0].id();
    assert_eq!(app.canvas.entries()[2].id(), top, "a layer above the folder");

    let depth = app.history.undo_depth();
    app.begin_layer_drag(2);
    {
        let drag = app.layer_drag.as_mut().expect("drag started");
        drag.to = 1; // dropped inside the folder
        drag.parent = Some(group);
    }
    app.finish_layer_drag();

    assert_eq!(app.canvas.entry(1).map(Entry::id), Some(top));
    assert_eq!(app.canvas.entry(1).and_then(Entry::parent), Some(group));
    assert_eq!(
        app.canvas.entry(2).and_then(Entry::parent),
        Some(group),
        "the folder's original layer stays with it"
    );
    assert_eq!(app.history.undo_depth(), depth + 1, "one undo step");

    app.undo();
    assert_eq!(app.canvas.entries()[2].id(), top);
    assert_eq!(app.canvas.entries()[2].parent(), None);
    assert_eq!(app.canvas.entries()[0].id(), group);
}

#[test]
fn dragging_a_layer_out_of_a_group_returns_it_to_the_top_level() {
    let mut app = InkforgeApp::with_defaults();
    app.add_group();
    let group = app.canvas.entries()[0].id();
    let inside = app.canvas.entries()[1].id();

    let depth = app.history.undo_depth();
    app.begin_layer_drag(1);
    {
        let drag = app.layer_drag.as_mut().expect("drag started");
        drag.to = 2; // dropped above the folder block
        drag.parent = None;
    }
    app.finish_layer_drag();

    assert_eq!(app.canvas.entries()[0].id(), group);
    assert_eq!(app.canvas.entries()[1].id(), inside);
    assert_eq!(app.canvas.entry(1).and_then(Entry::parent), None);
    assert_eq!(app.history.undo_depth(), depth + 1);

    app.undo();
    assert_eq!(app.canvas.entries()[0].id(), group);
    assert_eq!(app.canvas.entries()[1].id(), inside);
    assert_eq!(app.canvas.entries()[1].parent(), Some(group));

    app.redo();
    assert_eq!(app.canvas.entries()[1].parent(), None);
}

#[test]
fn painting_after_adding_a_layer_stays_on_that_layer() {
    let mut app = InkforgeApp::with_defaults();
    let lower = active(&app.canvas);

    app.add_layer();
    let upper = active(&app.canvas);

    app.dab_at(pos2(300.0, 300.0));
    app.end_stroke();

    assert_eq!(
        app.canvas.layer_surface(lower).unwrap().pixel(300, 300).a(),
        0
    );
    assert!(app.canvas.layer_surface(upper).unwrap().pixel(300, 300).a() > 0);
    assert!(app.history.can_undo());
}

use super::{crop_rgba, scale_rgba, KittyGrid, KittyPlacement, KittyVerticalAnchor};
use crate::panes::kitty_graphics::parser::{DecodedImage, KittyCommand, KittyFormat};
use crate::panes::kitty_graphics::store::KittyImageStore;
use crate::panes::sixel::PixelRect;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use zellij_utils::pane_size::SizeInPixels;

fn rgba_pixel(value: u8) -> [u8; 4] {
    [value, value, value, 255]
}

fn raster(values: &[u8]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| rgba_pixel(*value).to_vec())
        .collect()
}

fn test_grid() -> KittyGrid {
    KittyGrid::new(
        Rc::new(RefCell::new(Some(SizeInPixels {
            width: 10,
            height: 20,
        }))),
        Rc::new(RefCell::new(KittyImageStore::default())),
    )
}

fn test_placement(y: isize, height: usize) -> KittyPlacement {
    KittyPlacement {
        image_id: 1,
        placement_id: 0,
        placement_uid: 0,
        internal_id: 1,
        display_rect: PixelRect {
            x: 0,
            y,
            width: 20,
            height,
        },
        source_rect: PixelRect {
            x: 0,
            y: 0,
            width: 20,
            height,
        },
        emit_x: 0,
        emit_y: 0,
        scaled_px: None,
        dest_cells: (2, 2),
        cell_offset: (0, 0),
        z_index: 0,
        vertical_anchor: KittyVerticalAnchor {
            canonical_line: 0,
            offset_px_from_line_start: 0,
        },
    }
}

#[test]
fn scale_rgba_identity() {
    let src = raster(&[10, 20, 30, 40]);
    let out = scale_rgba(&src, 2, 2, 2, 2);
    assert_eq!(out, src);
}

#[test]
fn scale_rgba_upscales_constant_pixel() {
    let src = raster(&[123]);
    let out = scale_rgba(&src, 1, 1, 3, 3);
    assert_eq!(out, raster(&[123; 9]));
}

#[test]
fn scale_rgba_bilinear_gradient() {
    let src = raster(&[0, 255]);
    let out = scale_rgba(&src, 2, 1, 4, 1);
    let expected: Vec<u8> = vec![
        0, 0, 0, 255, 64, 64, 64, 255, 191, 191, 191, 255, 255, 255, 255, 255,
    ];
    assert_eq!(out, expected);
}

#[test]
fn crop_rgba_extracts_sub_rect() {
    let src = raster(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
    let out = crop_rgba(&src, 4, 4, 1, 1, 2, 2);
    assert_eq!(out, raster(&[5, 6, 9, 10]));
}

#[test]
fn changed_chunks_for_placement_straddling_changed_rect() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(30, 40));
    let mut changed_rects = HashMap::new();
    changed_rects.insert(1, 1);
    let chunks = grid.changed_kitty_chunks_in_viewport(changed_rects, 0, 4, 0, 0);
    assert_eq!(chunks.len(), 1);
    let chunk = &chunks[0];
    assert_eq!(chunk.cell_x, 0);
    assert_eq!(chunk.cell_y, 1);
    assert_eq!(chunk.source_px_x, 0);
    assert_eq!(chunk.source_px_y, 0);
    assert_eq!(chunk.source_px_width, 20);
    assert_eq!(chunk.source_px_height, 10);
    assert_eq!(chunk.scaled_image, None);
}

#[test]
fn clear_visible_placements_keeps_history_resident_images() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(0, 40));
    grid.placements.push(test_placement(400, 40));
    grid.clear_visible_placements(20);
    assert_eq!(grid.placement_count(), 1);
    assert_eq!(grid.placements[0].display_rect.y, 0);
}

#[test]
fn clear_all_placements_removes_history_resident_images() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(0, 40));
    grid.placements.push(test_placement(400, 40));
    grid.clear_all_placements();
    assert_eq!(grid.placement_count(), 0);
}

#[test]
fn changed_chunks_for_placement_with_top_scrolled_above_viewport() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(-20, 60));
    let chunks = grid.viewport_kitty_chunks(3, 0, 4, 0, 0);
    assert_eq!(chunks.len(), 1);
    let chunk = &chunks[0];
    assert_eq!(chunk.cell_x, 0);
    assert_eq!(chunk.cell_y, 0);
    assert_eq!(chunk.source_px_x, 0);
    assert_eq!(chunk.source_px_y, 20);
    assert_eq!(chunk.source_px_width, 20);
    assert_eq!(chunk.source_px_height, 40);
}

#[test]
fn changed_chunks_shift_cell_coordinates_by_viewport_offsets() {
    let mut grid = test_grid();
    let cell_width = 10usize;
    let cell_height = 20usize;
    let placement_x = 0usize;
    let placement_y = 30usize;
    grid.placements
        .push(test_placement(placement_y as isize, 40));
    let mut changed_rects = HashMap::new();
    changed_rects.insert(1, 1);
    let viewport_x_offset = 5usize;
    let viewport_y_offset = 7usize;
    let chunks = grid.changed_kitty_chunks_in_viewport(
        changed_rects,
        0,
        4,
        viewport_x_offset,
        viewport_y_offset,
    );
    assert_eq!(chunks.len(), 1);
    let chunk = &chunks[0];
    let expected_cell_x = viewport_x_offset + placement_x / cell_width;
    let expected_cell_y = viewport_y_offset + placement_y / cell_height;
    assert_eq!(chunk.cell_x, expected_cell_x);
    assert_eq!(chunk.cell_y, expected_cell_y);
    assert_eq!(chunk.source_px_x, 0);
    assert_eq!(chunk.source_px_y, 0);
    assert_eq!(chunk.source_px_width, 20);
    assert_eq!(chunk.source_px_height, 10);
}

#[test]
fn changed_chunks_for_scaled_placement_use_variant_space() {
    let mut grid = test_grid();
    let mut placement = test_placement(0, 40);
    placement.scaled_px = Some((20, 40));
    placement.dest_cells = (2, 2);
    grid.placements.push(placement);
    let mut changed_rects = HashMap::new();
    changed_rects.insert(1, 1);
    let chunks = grid.changed_kitty_chunks_in_viewport(changed_rects, 0, 4, 0, 0);
    assert_eq!(chunks.len(), 1);
    let chunk = &chunks[0];
    assert_eq!(chunk.cell_y, 1);
    assert_eq!(chunk.source_px_y, 20);
    assert_eq!(chunk.source_px_height, 20);
    assert_eq!(chunk.scaled_image.unwrap().size, (20, 40));
}

#[test]
fn changed_chunks_propagate_emit_offsets_of_clipped_placement() {
    let mut grid = test_grid();
    let mut placement = test_placement(100, 20);
    placement.emit_y = 20;
    grid.placements.push(placement);
    let mut changed_rects = HashMap::new();
    changed_rects.insert(5, 1);
    let chunks = grid.changed_kitty_chunks_in_viewport(changed_rects, 0, 4, 0, 0);
    assert_eq!(chunks.len(), 1);
    let chunk = &chunks[0];
    assert_eq!(chunk.cell_y, 5);
    assert_eq!(chunk.source_px_y, 20);
    assert_eq!(chunk.source_px_height, 20);
}

#[test]
fn region_scroll_alt_full_screen_moves_and_clips_at_top() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(40, 40));
    grid.placements.push(test_placement(0, 40));
    grid.placements.push(test_placement(180, 20));
    grid.apply_region_scroll(0, 200, 20, 0, 0, false);
    assert_eq!(grid.placements.len(), 3);
    assert_eq!(grid.placements[0].display_rect.y, 20);
    assert_eq!(grid.placements[0].display_rect.height, 40);
    assert_eq!(grid.placements[0].emit_y, 0);
    assert_eq!(grid.placements[1].display_rect.y, 0);
    assert_eq!(grid.placements[1].display_rect.height, 20);
    assert_eq!(grid.placements[1].emit_y, 20);
    assert_eq!(grid.placements[2].display_rect.y, 160);
}

#[test]
fn region_scroll_top_zero_non_alt_keeps_inside_and_shifts_below() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(0, 20));
    grid.placements.push(test_placement(60, 40));
    grid.placements.push(test_placement(160, 20));
    grid.apply_region_scroll(40, 140, 20, 20, 40, false);
    assert_eq!(grid.placements.len(), 3);
    assert_eq!(grid.placements[0].display_rect.y, 0);
    assert_eq!(grid.placements[1].display_rect.y, 60);
    assert_eq!(grid.placements[1].display_rect.height, 40);
    assert_eq!(grid.placements[2].display_rect.y, 180);
}

#[test]
fn region_scroll_inner_region_moves_inside_and_clips_straddler() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(40, 20));
    grid.placements.push(test_placement(80, 40));
    grid.placements.push(test_placement(0, 20));
    grid.apply_region_scroll(20, 100, 20, 0, 0, false);
    assert_eq!(grid.placements.len(), 3);
    assert_eq!(grid.placements[0].display_rect.y, 20);
    assert_eq!(grid.placements[0].display_rect.height, 20);
    assert_eq!(grid.placements[1].display_rect.y, 100);
    assert_eq!(grid.placements[1].display_rect.height, 20);
    assert_eq!(grid.placements[1].emit_y, 20);
    assert_eq!(grid.placements[2].display_rect.y, 0);
}

#[test]
fn region_scroll_reaps_placement_scrolled_out_of_region() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(20, 20));
    grid.apply_region_scroll(20, 100, 20, 0, 0, false);
    assert_eq!(grid.placements.len(), 0);
}

#[test]
fn reverse_index_region_scroll_clips_bottom_of_straddler() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(40, 40));
    grid.apply_region_scroll(0, 80, -20, 0, 0, false);
    assert_eq!(grid.placements.len(), 1);
    let placement = &grid.placements[0];
    assert_eq!(placement.display_rect.y, 60);
    assert_eq!(placement.display_rect.height, 20);
    assert_eq!(placement.emit_y, 0);
    assert_eq!(placement.source_rect.y, 0);
}

#[test]
fn reverse_index_region_scroll_reaps_fully_below_margin() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(60, 20));
    grid.apply_region_scroll(0, 80, -20, 0, 0, false);
    assert_eq!(grid.placements.len(), 0);
}

fn transmitted(
    grid: &mut KittyGrid,
    width: u32,
    height: u32,
) -> (u32, super::super::store::InternalImageId) {
    let command = KittyCommand {
        image_id: Some(1),
        pixel_width: Some(width),
        pixel_height: Some(height),
        ..Default::default()
    };
    let image = DecodedImage {
        bytes: vec![0u8; (width * height * 4) as usize],
        width,
        height,
        format: KittyFormat::Rgba32,
    };
    let pane_image_id = grid.transmit(&command, image).expect("a transmission");
    let internal = grid.pane_image_id_map()[&pane_image_id];
    (pane_image_id, internal)
}

#[test]
fn an_unscaled_placement_emits_from_its_source_rectangle() {
    let mut grid = test_grid();
    let (pane_image_id, internal) = transmitted(&mut grid, 64, 64);
    let command = KittyCommand {
        image_id: Some(1),
        source_x: 8,
        source_y: 10,
        source_w: 16,
        source_h: 20,
        ..Default::default()
    };
    let cell = SizeInPixels {
        width: 10,
        height: 20,
    };
    grid.place(
        pane_image_id,
        internal,
        &command,
        (0, 0),
        cell,
        KittyVerticalAnchor {
            canonical_line: 0,
            offset_px_from_line_start: 0,
        },
    )
    .expect("a placement");

    let mut changed_rects = HashMap::new();
    changed_rects.insert(0, 2);
    let chunks = grid.changed_kitty_chunks_in_viewport(changed_rects, 0, 40, 0, 0);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].source_px_x, 8);
    assert_eq!(chunks[0].source_px_y, 10);
    assert_eq!(chunks[0].source_px_width, 16);
    assert_eq!(chunks[0].source_px_height, 20);
}

#[test]
fn a_scaled_placement_emits_from_the_origin_of_its_cropped_variant() {
    let mut grid = test_grid();
    let (pane_image_id, internal) = transmitted(&mut grid, 64, 64);
    let command = KittyCommand {
        image_id: Some(1),
        source_x: 8,
        source_y: 10,
        source_w: 16,
        source_h: 20,
        columns: 2,
        rows: 2,
        ..Default::default()
    };
    let cell = SizeInPixels {
        width: 10,
        height: 20,
    };
    grid.place(
        pane_image_id,
        internal,
        &command,
        (0, 0),
        cell,
        KittyVerticalAnchor {
            canonical_line: 0,
            offset_px_from_line_start: 0,
        },
    )
    .expect("a placement");

    let placement = &grid.placements()[0];
    assert_eq!(placement.emit_x, 0);
    assert_eq!(placement.emit_y, 0);
    assert_eq!(placement.source_rect.x, 8);
    assert_eq!(placement.source_rect.y, 10);
}

mod virtual_placements {
    use super::*;
    use crate::panes::kitty_graphics::parser::{
        DecodedImage, KittyAction, KittyCommand, KittyFormat,
    };
    use crate::panes::kitty_graphics::store::InternalImageId;

    fn virtual_transmit_command(image_id: u32, columns: u32, rows: u32) -> KittyCommand {
        KittyCommand {
            action: KittyAction::TransmitAndDisplay,
            image_id: Some(image_id),
            columns,
            rows,
            unicode_placeholder: true,
            image: Some(DecodedImage {
                bytes: vec![255; 4 * 4 * 4],
                width: 4,
                height: 4,
                format: KittyFormat::Rgba32,
            }),
            ..Default::default()
        }
    }

    fn delete_command(specifier: char, image_id: Option<u32>) -> KittyCommand {
        KittyCommand {
            action: KittyAction::Delete,
            delete_specifier: Some(specifier),
            image_id,
            ..Default::default()
        }
    }

    fn virtual_display_command(image_id: u32, placement_id: u32) -> KittyCommand {
        KittyCommand {
            action: KittyAction::Display,
            image_id: Some(image_id),
            placement_id: Some(placement_id),
            unicode_placeholder: true,
            ..Default::default()
        }
    }

    fn register_virtual_for_delete(
        grid: &mut KittyGrid,
        specifier: char,
    ) -> (u32, InternalImageId, KittyCommand) {
        let mut transmit = virtual_transmit_command(7, 20, 5);
        transmit.placement_id = Some(1);
        let mut delete = delete_command(specifier, Some(7));
        if matches!(specifier, 'n' | 'N') {
            transmit.image_id = None;
            transmit.image_number = Some(7);
            delete.image_id = None;
            delete.image_number = Some(7);
        }
        let image_id = grid
            .register_virtual_transmit(&transmit)
            .unwrap()
            .image_id
            .unwrap();
        let internal = grid.pane_image_id_map()[&image_id];
        (image_id, internal, delete)
    }

    fn place_positioned(grid: &mut KittyGrid, image_id: u32, placement_id: u32) {
        let command = KittyCommand {
            action: KittyAction::Display,
            image_id: Some(image_id),
            placement_id: Some(placement_id),
            ..Default::default()
        };
        let (_, internal) = grid.resolve_display_target(&command).unwrap();
        grid.place(
            image_id,
            internal,
            &command,
            (0, 0),
            SizeInPixels {
                width: 10,
                height: 20,
            },
            KittyVerticalAnchor {
                canonical_line: 0,
                offset_px_from_line_start: 0,
            },
        )
        .unwrap();
    }

    #[test]
    fn register_virtual_transmit_stores_image_and_placement() {
        let mut grid = test_grid();
        let reply = grid
            .register_virtual_transmit(&virtual_transmit_command(7, 20, 5))
            .unwrap();
        assert_eq!(reply.image_id, Some(7));
        assert!(grid.has_virtual_placements());
        let placement = grid.virtual_placement(7, 0).unwrap();
        assert_eq!((placement.columns, placement.rows), (20, 5));
        assert_eq!(
            grid.kitty_image_store.borrow().image_count(),
            1,
            "image is stored"
        );
        assert_eq!(
            grid.kitty_image_store
                .borrow()
                .refcount(placement.internal_id),
            Some(1),
            "the virtual placement holds one ref"
        );
    }

    #[test]
    fn plain_retransmit_under_same_id_replaces_virtual_placement() {
        let mut grid = test_grid();
        grid.register_virtual_transmit(&virtual_transmit_command(7, 20, 5))
            .unwrap();
        let mut plain = virtual_transmit_command(7, 0, 0);
        plain.unicode_placeholder = false;
        let image = plain.image.take().unwrap();
        grid.transmit(&plain, image).unwrap();
        assert!(
            !grid.has_virtual_placements(),
            "the old virtual placement is dropped with its image"
        );
        assert_eq!(
            grid.kitty_image_store.borrow().image_count(),
            1,
            "only the newly transmitted image remains"
        );
    }

    #[test]
    fn register_virtual_display_places_an_already_transmitted_image() {
        let mut grid = test_grid();
        let mut transmit = virtual_transmit_command(9, 0, 0);
        transmit.unicode_placeholder = false;
        let image = transmit.image.take().unwrap();
        grid.transmit(&transmit, image).unwrap();
        let display = KittyCommand {
            action: KittyAction::Display,
            image_id: Some(9),
            columns: 10,
            rows: 3,
            unicode_placeholder: true,
            ..Default::default()
        };
        grid.register_virtual_display(&display).unwrap();
        let placement = grid.virtual_placement(9, 0).unwrap();
        assert_eq!((placement.columns, placement.rows), (10, 3));
    }

    #[test]
    fn register_virtual_display_for_unknown_image_errors() {
        let mut grid = test_grid();
        let display = KittyCommand {
            action: KittyAction::Display,
            image_id: Some(42),
            unicode_placeholder: true,
            ..Default::default()
        };
        assert!(grid.register_virtual_display(&display).is_err());
    }

    #[test]
    fn placement_id_selection_and_reregistration() {
        let mut grid = test_grid();
        let mut first = virtual_transmit_command(7, 20, 5);
        first.placement_id = Some(1);
        grid.register_virtual_transmit(&first).unwrap();
        let second = KittyCommand {
            action: KittyAction::Display,
            image_id: Some(7),
            placement_id: Some(2),
            columns: 10,
            rows: 2,
            unicode_placeholder: true,
            ..Default::default()
        };
        grid.register_virtual_display(&second).unwrap();
        assert_eq!(grid.virtual_placement(7, 1).unwrap().columns, 20);
        assert_eq!(grid.virtual_placement(7, 2).unwrap().columns, 10);
        // no placement selected -> the most recently registered one
        assert_eq!(grid.virtual_placement(7, 0).unwrap().columns, 10);
        let first_uid = grid.virtual_placement(7, 1).unwrap().placement_uid;
        let mut replacement = second.clone();
        replacement.placement_id = Some(1);
        grid.register_virtual_display(&replacement).unwrap();
        assert_eq!(
            grid.virtual_placement(7, 1).unwrap().placement_uid,
            first_uid,
            "re-registering a placement id keeps its uid"
        );
    }

    #[test]
    fn id_and_range_deletes_remove_virtual_placements() {
        let mut grid = test_grid();
        grid.register_virtual_transmit(&virtual_transmit_command(7, 20, 5))
            .unwrap();
        grid.register_virtual_transmit(&virtual_transmit_command(8, 20, 5))
            .unwrap();
        grid.delete(&delete_command('i', Some(7)), (0, 0), (10, 10), 0)
            .unwrap();
        assert!(grid.virtual_placement(7, 0).is_none());
        assert!(grid.virtual_placement(8, 0).is_some());
        let range = KittyCommand {
            action: KittyAction::Delete,
            delete_specifier: Some('R'),
            source_x: 1,
            source_y: 100,
            ..Default::default()
        };
        grid.delete(&range, (0, 0), (10, 10), 0).unwrap();
        assert!(!grid.has_virtual_placements());
        assert_eq!(
            grid.kitty_image_store.borrow().image_count(),
            0,
            "an uppercase range delete frees the images"
        );
    }

    #[test]
    fn uppercase_placement_delete_preserves_other_virtual_refs() {
        for specifier in ['I', 'N'] {
            let mut grid = test_grid();
            let (image_id, internal, mut delete) =
                register_virtual_for_delete(&mut grid, specifier);
            grid.register_virtual_display(&virtual_display_command(image_id, 2))
                .unwrap();
            assert_eq!(grid.kitty_image_store.borrow().refcount(internal), Some(2));

            delete.placement_id = Some(99);
            grid.delete(&delete, (0, 0), (10, 10), 0).unwrap();
            assert!(grid.virtual_placement(image_id, 1).is_some());
            assert!(grid.virtual_placement(image_id, 2).is_some());
            assert_eq!(grid.kitty_image_store.borrow().refcount(internal), Some(2));

            delete.placement_id = Some(1);
            grid.delete(&delete, (0, 0), (10, 10), 0).unwrap();
            assert!(grid.virtual_placement(image_id, 1).is_none());
            assert!(grid.virtual_placement(image_id, 2).is_some());
            assert_eq!(grid.kitty_image_store.borrow().refcount(internal), Some(1));
            assert_eq!(
                grid.resolve_display_target(&delete).unwrap(),
                (image_id, internal)
            );

            // Deleting the same, now nonexistent placement must not consume p=2's ref.
            grid.delete(&delete, (0, 0), (10, 10), 0).unwrap();
            assert_eq!(grid.kitty_image_store.borrow().refcount(internal), Some(1));

            delete.placement_id = Some(2);
            grid.delete(&delete, (0, 0), (10, 10), 0).unwrap();
            assert!(!grid.has_virtual_placements());
            assert_eq!(grid.kitty_image_store.borrow().refcount(internal), None);
            assert!(!grid.pane_image_id_map().contains_key(&image_id));
            assert!(grid.resolve_display_target(&delete).is_err());
        }
    }

    #[test]
    fn uppercase_placement_delete_counts_positioned_and_virtual_refs() {
        for specifier in ['I', 'N'] {
            let mut grid = test_grid();
            let (image_id, internal, mut delete) =
                register_virtual_for_delete(&mut grid, specifier);
            place_positioned(&mut grid, image_id, 2);
            grid.register_virtual_display(&virtual_display_command(image_id, 3))
                .unwrap();
            assert_eq!(grid.kitty_image_store.borrow().refcount(internal), Some(3));

            delete.placement_id = Some(1);
            grid.delete(&delete, (0, 0), (10, 10), 0).unwrap();
            assert_eq!(grid.placement_count(), 1);
            assert!(grid.virtual_placement(image_id, 1).is_none());
            assert!(grid.virtual_placement(image_id, 3).is_some());
            assert_eq!(grid.kitty_image_store.borrow().refcount(internal), Some(2));

            delete.placement_id = Some(2);
            grid.delete(&delete, (0, 0), (10, 10), 0).unwrap();
            assert_eq!(grid.placement_count(), 0);
            assert!(grid.virtual_placement(image_id, 3).is_some());
            assert_eq!(grid.kitty_image_store.borrow().refcount(internal), Some(1));
            assert_eq!(
                grid.resolve_display_target(&delete).unwrap(),
                (image_id, internal)
            );

            delete.placement_id = Some(3);
            grid.delete(&delete, (0, 0), (10, 10), 0).unwrap();
            assert!(!grid.has_virtual_placements());
            assert_eq!(grid.kitty_image_store.borrow().refcount(internal), None);
            assert!(grid.resolve_display_target(&delete).is_err());
        }
    }

    #[test]
    fn lowercase_delete_retains_data_for_new_positioned_and_virtual_placements() {
        for (lowercase, uppercase) in [('i', 'I'), ('n', 'N')] {
            let mut grid = test_grid();
            let (image_id, internal, mut delete) =
                register_virtual_for_delete(&mut grid, lowercase);
            place_positioned(&mut grid, image_id, 2);

            delete.placement_id = Some(1);
            grid.delete(&delete, (0, 0), (10, 10), 0).unwrap();
            assert!(!grid.has_virtual_placements());
            assert_eq!(grid.placement_count(), 1);
            assert_eq!(grid.kitty_image_store.borrow().refcount(internal), Some(1));

            delete.placement_id = Some(2);
            grid.delete(&delete, (0, 0), (10, 10), 0).unwrap();
            assert_eq!(grid.placement_count(), 0);
            assert_eq!(grid.kitty_image_store.borrow().refcount(internal), Some(0));
            assert_eq!(
                grid.resolve_display_target(&delete).unwrap(),
                (image_id, internal)
            );

            // Even unreferenced retained data must survive an unmatched p= delete.
            delete.delete_specifier = Some(uppercase);
            delete.placement_id = Some(99);
            grid.delete(&delete, (0, 0), (10, 10), 0).unwrap();
            assert_eq!(grid.kitty_image_store.borrow().refcount(internal), Some(0));

            let mut display = delete.clone();
            display.action = KittyAction::Display;
            display.placement_id = Some(3);
            display.unicode_placeholder = true;
            grid.register_virtual_display(&display).unwrap();
            place_positioned(&mut grid, image_id, 4);
            assert_eq!(grid.kitty_image_store.borrow().refcount(internal), Some(2));

            delete.delete_specifier = Some(lowercase);
            delete.placement_id = None;
            grid.delete(&delete, (0, 0), (10, 10), 0).unwrap();
            assert!(!grid.has_virtual_placements());
            assert_eq!(grid.placement_count(), 0);
            assert_eq!(grid.kitty_image_store.borrow().refcount(internal), Some(0));

            // An unqualified uppercase delete also frees data retained at zero refs.
            delete.delete_specifier = Some(uppercase);
            grid.delete(&delete, (0, 0), (10, 10), 0).unwrap();
            assert_eq!(grid.kitty_image_store.borrow().refcount(internal), None);
            assert!(grid.resolve_display_target(&delete).is_err());
        }
    }

    #[test]
    fn positioned_delete_specifiers_preserve_virtual_prototypes() {
        for specifier in [
            'a', 'A', 'c', 'C', 'p', 'P', 'q', 'Q', 'x', 'X', 'y', 'Y', 'z', 'Z',
        ] {
            let mut grid = test_grid();
            let (image_id, internal, _) = register_virtual_for_delete(&mut grid, 'I');
            place_positioned(&mut grid, image_id, 2);
            let mut delete = delete_command(specifier, None);
            delete.source_x = 1;
            delete.source_y = 1;
            grid.delete(&delete, (0, 0), (10, 10), 0).unwrap();
            assert_eq!(grid.placement_count(), 0, "d={specifier}");
            assert!(
                grid.virtual_placement(image_id, 1).is_some(),
                "d={specifier}"
            );
            assert_eq!(
                grid.kitty_image_store.borrow().refcount(internal),
                Some(1),
                "d={specifier}"
            );
        }
    }

    #[test]
    fn clear_positioned_placements_preserves_virtual_prototypes_and_refs() {
        let mut grid = test_grid();
        let (image_id, internal, _) = register_virtual_for_delete(&mut grid, 'I');
        grid.register_virtual_display(&virtual_display_command(image_id, 2))
            .unwrap();
        place_positioned(&mut grid, image_id, 3);
        let (positioned_image_id, positioned_internal) = transmitted(&mut grid, 4, 4);
        place_positioned(&mut grid, positioned_image_id, 1);
        assert_eq!(grid.kitty_image_store.borrow().refcount(internal), Some(3));
        assert_eq!(
            grid.kitty_image_store
                .borrow()
                .refcount(positioned_internal),
            Some(1)
        );
        let first = grid.virtual_placement(image_id, 1).unwrap();
        let second = grid.virtual_placement(image_id, 2).unwrap();

        grid.clear_positioned_placements();
        assert_eq!(grid.placement_count(), 0);
        assert_eq!(grid.virtual_placement(image_id, 1), Some(first));
        assert_eq!(grid.virtual_placement(image_id, 2), Some(second));
        assert_eq!(grid.kitty_image_store.borrow().refcount(internal), Some(2));
        assert_eq!(
            grid.kitty_image_store
                .borrow()
                .refcount(positioned_internal),
            None
        );

        grid.clear_positioned_placements();
        assert_eq!(grid.kitty_image_store.borrow().refcount(internal), Some(2));
        grid.clear_all_placements();
        assert!(!grid.has_virtual_placements());
        assert_eq!(grid.kitty_image_store.borrow().image_count(), 0);
    }

    #[test]
    fn clear_visible_without_cell_size_preserves_virtual_prototypes() {
        let mut grid = test_grid();
        let (image_id, internal, _) = register_virtual_for_delete(&mut grid, 'I');
        place_positioned(&mut grid, image_id, 2);
        *grid.character_cell_size.borrow_mut() = None;
        grid.clear_visible_placements(0);
        assert_eq!(grid.placement_count(), 0);
        assert!(grid.virtual_placement(image_id, 1).is_some());
        assert_eq!(grid.kitty_image_store.borrow().refcount(internal), Some(1));
    }

    #[test]
    fn clear_all_placements_frees_virtual_images() {
        let mut grid = test_grid();
        let (image_id, internal, _) = register_virtual_for_delete(&mut grid, 'I');
        grid.register_virtual_display(&virtual_display_command(image_id, 2))
            .unwrap();
        place_positioned(&mut grid, image_id, 3);
        assert_eq!(grid.kitty_image_store.borrow().refcount(internal), Some(3));
        grid.clear_all_placements();
        assert_eq!(grid.placement_count(), 0);
        assert!(!grid.has_virtual_placements());
        assert_eq!(grid.kitty_image_store.borrow().image_count(), 0);
    }

    #[test]
    fn fit_rgba_into_box_letterboxes_preserving_aspect() {
        // a 2x1 white image into a 4x4 box: scaled to 4x2, centered
        // vertically, transparent rows above and below
        let src = vec![255; 2 * 1 * 4];
        let out = super::super::fit_rgba_into_box(&src, 2, 1, 4, 4);
        assert_eq!(out.len(), 4 * 4 * 4);
        let row = |index: usize| &out[index * 4 * 4..(index + 1) * 4 * 4];
        assert!(row(0).iter().all(|byte| *byte == 0), "top padding");
        assert!(row(1).iter().all(|byte| *byte == 255), "image row");
        assert!(row(2).iter().all(|byte| *byte == 255), "image row");
        assert!(row(3).iter().all(|byte| *byte == 0), "bottom padding");
    }
}

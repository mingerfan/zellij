//! Regression fixtures use synthetic pixels and the same protocol shape as Pi.
//! No conversation text, terminal process, or live user session is involved.
use super::*;
use crate::panes::kitty_graphics::{KITTY_DIACRITICS, PLACEHOLDER_CHAR};

const IMAGE_ID: u32 = 0x123456;

fn placeholders(columns: usize, row: usize) -> String {
    let mut text = String::from("\x1b[38;2;18;52;86m");
    for column in 0..columns {
        text.push(PLACEHOLDER_CHAR);
        text.push(char::from_u32(KITTY_DIACRITICS[row]).unwrap());
        text.push(char::from_u32(KITTY_DIACRITICS[column]).unwrap());
    }
    text.push_str("\x1b[39m");
    text
}

fn transmit(
    grid: &mut Grid,
    parser: &mut vt_parser::Parser,
    interceptor: &mut KittyApcInterceptor,
    columns: usize,
    rows: usize,
) {
    feed_kitty_bytes(
        grid,
        parser,
        interceptor,
        &kitty_apc(
            &format!("a=T,f=100,q=2,U=1,p=1,i={IMAGE_ID},c={columns},r={rows}"),
            &KITTY_PNG_2X2,
        ),
    );
}

#[test]
fn pi_png_placeholders_render_every_row_with_default_placement_selection() {
    for (columns, rows) in [(14, 1), (17, 2), (39, 2)] {
        for graphemes in [false, true] {
            let (mut grid, store) = new_kitty_grid(8, 80);
            *grid.character_cell_size.borrow_mut() = Some(SizeInPixels {
                width: 11,
                height: 24,
            });
            let mut parser = vt_parser::Parser::new();
            let mut interceptor = KittyApcInterceptor::new();
            if graphemes {
                feed_kitty_bytes(&mut grid, &mut parser, &mut interceptor, b"\x1b[?2027h");
            }
            transmit(&mut grid, &mut parser, &mut interceptor, columns, rows);
            assert_eq!(grid.cursor.x, 0, "virtual upload must not move the cursor");
            assert!(grid.pending_messages_to_pty.is_empty(), "q=2 is silent");
            for row in 0..rows {
                feed_kitty_bytes(
                    &mut grid,
                    &mut parser,
                    &mut interceptor,
                    format!("\x1b[{};1H{}", row + 1, placeholders(columns, row)).as_bytes(),
                );
            }
            let chunks = grid.placeholder_kitty_chunks(3, 4);
            assert_eq!(chunks.len(), rows, "one contiguous image slice per row");
            for (row, chunk) in chunks.iter().enumerate() {
                assert_eq!((chunk.cell_x, chunk.cell_y), (3, row + 4));
                assert_eq!((chunk.source_px_x, chunk.source_px_y), (0, row * 24));
                assert_eq!(
                    (chunk.source_px_width, chunk.source_px_height),
                    (columns * 11, 24)
                );
                let key = chunk.scaled_image.unwrap();
                assert!(key.fit);
                assert_eq!(
                    store
                        .borrow()
                        .scaled_variant(chunk.internal_image_id, key)
                        .unwrap()
                        .len(),
                    columns * rows * 11 * 24 * 4
                );
            }
            assert_eq!(grid.viewport[0].columns[0].character, PLACEHOLDER_CHAR);
            assert_eq!(grid.viewport[0].columns[0].code_point_count(), 3);
        }
    }
}

#[test]
fn chunked_png_virtual_upload_then_placeholder_works() {
    let (mut grid, _store) = new_kitty_grid(5, 20);
    let mut parser = vt_parser::Parser::new();
    let mut interceptor = KittyApcInterceptor::new();
    let encoded = BASE64_ENCODER.encode(KITTY_PNG_2X2);
    let split = 44;
    let bytes = format!(
        "\x1b_Ga=T,f=100,q=2,U=1,p=1,i={IMAGE_ID},c=14,r=1,m=1;{}\x1b\\\x1b_Gm=0,q=2;{}\x1b\\{}",
        &encoded[..split],
        &encoded[split..],
        placeholders(14, 0)
    );
    // Exercise split UTF-8 and APC input boundaries, not just direct helpers.
    for byte in bytes.as_bytes() {
        feed_kitty_bytes(&mut grid, &mut parser, &mut interceptor, &[*byte]);
    }
    assert_eq!(grid.placeholder_kitty_chunks(0, 0).len(), 1);
    assert!(grid.pending_messages_to_pty.is_empty());
}

#[test]
fn oversized_virtual_geometry_is_rejected_before_pixel_allocation() {
    for (columns, rows) in [
        (100_000, 100_000),
        (u32::MAX as usize, 0),
        (0, u32::MAX as usize),
    ] {
        let (mut grid, store) = new_kitty_grid(4, 10);
        *store.borrow_mut() = KittyImageStore::with_quota(1024);
        let mut parser = vt_parser::Parser::new();
        let mut interceptor = KittyApcInterceptor::new();
        transmit(&mut grid, &mut parser, &mut interceptor, columns, rows);
        feed_kitty_bytes(
            &mut grid,
            &mut parser,
            &mut interceptor,
            placeholders(1, 0).as_bytes(),
        );
        let before = store.borrow().total_bytes();
        for _ in 0..3 {
            assert!(grid.placeholder_kitty_chunks(0, 0).is_empty());
        }
        assert_eq!(store.borrow().total_bytes(), before);
        assert_eq!(before, 16, "only the 2x2 source image was allocated");
    }
}

#[test]
fn alternate_screen_shrink_clips_placeholder_slices_to_pane_width() {
    let (mut grid, _store) = new_kitty_grid(4, 20);
    let mut parser = vt_parser::Parser::new();
    let mut interceptor = KittyApcInterceptor::new();
    feed_kitty_bytes(&mut grid, &mut parser, &mut interceptor, b"\x1b[?1049h");
    transmit(&mut grid, &mut parser, &mut interceptor, 20, 1);
    feed_kitty_bytes(
        &mut grid,
        &mut parser,
        &mut interceptor,
        placeholders(20, 0).as_bytes(),
    );
    grid.change_size(4, 5);
    let chunks = grid.placeholder_kitty_chunks(7, 0);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].source_px_width, 5 * 10);
    assert!(chunks
        .iter()
        .all(|chunk| chunk.cell_x + chunk.source_px_width / 10 <= 7 + 5));
}

#[test]
fn erase_scrollback_keeps_visible_virtual_images_but_erased_text_hides_them() {
    let (mut grid, _store) = new_kitty_grid(4, 20);
    let mut parser = vt_parser::Parser::new();
    let mut interceptor = KittyApcInterceptor::new();
    transmit(&mut grid, &mut parser, &mut interceptor, 14, 1);
    feed_kitty_bytes(
        &mut grid,
        &mut parser,
        &mut interceptor,
        placeholders(14, 0).as_bytes(),
    );
    assert_eq!(grid.placeholder_kitty_chunks(0, 0).len(), 1);
    feed_kitty_bytes(&mut grid, &mut parser, &mut interceptor, b"\x1b[3J");
    assert_eq!(grid.placeholder_kitty_chunks(0, 0).len(), 1);
    feed_kitty_bytes(
        &mut grid,
        &mut parser,
        &mut interceptor,
        b"\x1b[1;1H\x1b[2K",
    );
    assert!(grid.placeholder_kitty_chunks(0, 0).is_empty());
    assert!(grid.kitty_grid.has_virtual_placements());
}

#[test]
fn fitted_placeholder_and_stretched_normal_placement_have_distinct_caches() {
    let (mut grid, store) = new_kitty_grid(4, 20);
    let mut parser = vt_parser::Parser::new();
    let mut interceptor = KittyApcInterceptor::new();
    feed_kitty_bytes(
        &mut grid,
        &mut parser,
        &mut interceptor,
        &kitty_apc(
            &format!("a=T,f=100,q=2,C=1,i={IMAGE_ID},c=4,r=1"),
            &KITTY_PNG_2X2,
        ),
    );
    feed_kitty_bytes(
        &mut grid,
        &mut parser,
        &mut interceptor,
        &kitty_apc(&format!("a=p,q=2,U=1,p=1,i={IMAGE_ID},c=4,r=1"), &[]),
    );
    feed_kitty_bytes(
        &mut grid,
        &mut parser,
        &mut interceptor,
        placeholders(4, 0).as_bytes(),
    );
    let chunks = grid.placeholder_kitty_chunks(0, 0);
    assert_eq!(chunks.len(), 1);
    let key = chunks[0].scaled_image.unwrap();
    let mut ordinary = key;
    ordinary.fit = false;
    let store = store.borrow();
    assert_ne!(
        store
            .scaled_variant(chunks[0].internal_image_id, key)
            .unwrap(),
        store
            .scaled_variant(chunks[0].internal_image_id, ordinary)
            .unwrap()
    );
}

#[test]
fn ordinary_combining_text_still_survives_next_to_placeholders() {
    let (mut grid, _store) = new_kitty_grid(4, 20);
    let mut parser = vt_parser::Parser::new();
    let mut interceptor = KittyApcInterceptor::new();
    feed_kitty_bytes(
        &mut grid,
        &mut parser,
        &mut interceptor,
        "e\u{301}中".as_bytes(),
    );
    feed_kitty_bytes(
        &mut grid,
        &mut parser,
        &mut interceptor,
        placeholders(2, 0).as_bytes(),
    );
    feed_kitty_bytes(
        &mut grid,
        &mut parser,
        &mut interceptor,
        "e\u{301}".as_bytes(),
    );
    assert_eq!(grid.viewport[0].columns[0].text(), "e\u{301}");
    assert_eq!(grid.viewport[0].columns[1].character, '中');
    assert_eq!(grid.viewport[0].columns[4].text(), "e\u{301}");
}

use super::super::placeholders::*;
use crate::panes::terminal_character::TerminalCharacter;

fn cell(marks: &[usize]) -> TerminalCharacter {
    let mut cell = TerminalCharacter::new(PLACEHOLDER_CHAR);
    for &index in marks {
        assert!(cell.attach(char::from_u32(KITTY_DIACRITICS[index]).unwrap()));
    }
    cell
}

#[test]
fn table_is_sorted_for_binary_search() {
    for window in KITTY_DIACRITICS.windows(2) {
        assert!(window[0] < window[1]);
    }
}

#[test]
fn explicit_coordinates_use_attached_marks_not_private_use_codepoints() {
    let original = cell(&[3, 7, 255]);
    assert_eq!(original.character, PLACEHOLDER_CHAR);
    assert_eq!(
        decode_placeholder(&original, None),
        Some(PlaceholderCell {
            image_row: 3,
            image_column: 7,
            image_id_msb: 255,
        })
    );
    assert_eq!(
        decode_placeholder(&TerminalCharacter::new('\u{100000}'), None),
        None
    );
    assert!(!is_placeholder_character('\u{100000}'));
}

#[test]
fn omitted_coordinates_inherit_only_from_matching_row_and_column() {
    let left = Some(PlaceholderCell {
        image_row: 2,
        image_column: 4,
        image_id_msb: 7,
    });
    assert_eq!(
        decode_placeholder(&cell(&[]), left),
        Some(PlaceholderCell {
            image_row: 2,
            image_column: 5,
            image_id_msb: 7,
        })
    );
    assert_eq!(
        decode_placeholder(&cell(&[2]), left).unwrap().image_column,
        5
    );
    assert_eq!(
        decode_placeholder(&cell(&[1]), left),
        Some(PlaceholderCell {
            image_row: 1,
            image_column: 0,
            image_id_msb: 0,
        })
    );
    assert_eq!(
        decode_placeholder(&cell(&[2, 5]), left)
            .unwrap()
            .image_id_msb,
        7
    );
    assert_eq!(
        decode_placeholder(&cell(&[2, 6]), left)
            .unwrap()
            .image_id_msb,
        0
    );
    assert_eq!(
        decode_placeholder(&cell(&[1, 5]), left)
            .unwrap()
            .image_id_msb,
        0
    );
}

#[test]
fn invalid_high_byte_or_extra_marks_are_not_decoded() {
    assert_eq!(decode_placeholder(&cell(&[0, 0, 256]), None), None);
    assert_eq!(decode_placeholder(&cell(&[0, 0, 0, 0]), None), None);
}

#[test]
fn diacritic_lookup() {
    assert_eq!(diacritic_index('\u{0305}'), Some(0));
    assert_eq!(diacritic_index('\u{030D}'), Some(1));
    assert_eq!(diacritic_index('\u{1D244}'), Some(297));
    assert_eq!(diacritic_index('a'), None);
}

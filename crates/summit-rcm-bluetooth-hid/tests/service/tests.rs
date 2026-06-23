use super::decode_hid_character;

#[test]
fn decodes_lowercase_hid_characters() {
    assert_eq!(decode_hid_character(4, false), Some('a'));
    assert_eq!(decode_hid_character(30, false), Some('1'));
    assert_eq!(decode_hid_character(56, false), Some('/'));
}

#[test]
fn decodes_shifted_hid_characters() {
    assert_eq!(decode_hid_character(4, true), Some('A'));
    assert_eq!(decode_hid_character(30, true), Some('!'));
    assert_eq!(decode_hid_character(47, true), Some('{'));
}

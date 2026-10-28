//! Digests as text: [`hex`] writes the lowercase hexadecimal form of a checksum, and [`is_hex`] checks that a pinned commit id or checksum has it.

/// The lowercase hexadecimal digits of `bytes`.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0x0f])
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .collect()
}

/// Whether `text` is `length` digits of lowercase hexadecimal.
pub(crate) fn is_hex(text: &str, length: usize) -> bool {
    text.len() == length
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::{hex, is_hex};

    #[test]
    fn hex_writes_two_lowercase_digits_per_byte() {
        assert_eq!(hex(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
        assert_eq!(hex(&[]), "");
    }

    #[test]
    fn only_lowercase_hexadecimal_of_the_given_length_is_accepted() {
        let commit = "0123456789abcdef0123456789abcdef01234567";
        assert!(is_hex(commit, 40));
        assert!(!is_hex(commit, 41), "one digit short");
        assert!(!is_hex(&"a".repeat(41), 40), "one digit too many");
        assert!(!is_hex(&commit.to_uppercase(), 40), "upper case");
        let not_hex = format!("{}g", "a".repeat(39));
        assert!(!is_hex(&not_hex, 40), "a letter after f");
    }
}

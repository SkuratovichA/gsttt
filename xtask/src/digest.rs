//! Digests as text: [`is_hex`] checks that a pinned commit id or checksum is lowercase hexadecimal, the form [`faster_hex::hex_string`] writes.

/// Whether `text` is `length` digits of lowercase hexadecimal.
pub(crate) fn is_hex(text: &str, length: usize) -> bool {
    text.len() == length
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::is_hex;

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

//! Digests as text: [`is_hex`] checks that a pinned commit id or checksum is lowercase hexadecimal, the form [`faster_hex::hex_string`] writes, and [`sha256_hex`] writes one.

use sha2::{Digest as _, Sha256};

/// The SHA-256 of `bytes` as 64 lower-case hexadecimal digits.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    faster_hex::hex_string(&Sha256::digest(bytes))
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
    use super::{is_hex, sha256_hex};

    #[test]
    fn the_digest_of_a_text_is_sixty_four_lowercase_hexadecimal_digits() {
        let digest = sha256_hex(b"abc");
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(is_hex(&digest, 64));
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

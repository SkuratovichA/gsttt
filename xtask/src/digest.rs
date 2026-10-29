//! Digests as text: [`is_hex`] checks that a pinned commit id or checksum is lowercase hexadecimal, the form [`faster_hex::hex_string`] writes, and [`sha256_hex`] and [`sha256_file`] write one.

use std::fs::File;
use std::io::{BufRead as _, BufReader};
use std::path::Path;

use anyhow::Context as _;
use sha2::{Digest as _, Sha256};

/// The SHA-256 of `bytes` as 64 lower-case hexadecimal digits.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    faster_hex::hex_string(&Sha256::digest(bytes))
}

/// The SHA-256 of the file at `path` as 64 lower-case hexadecimal digits, read in pieces: an archive or a binary is too large to hold in memory twice.
pub(crate) fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let input = File::open(path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut reader = BufReader::new(input);
    let mut hasher = Sha256::new();
    loop {
        let piece = reader
            .fill_buf()
            .with_context(|| format!("cannot read {}", path.display()))?;
        if piece.is_empty() {
            break;
        }
        hasher.update(piece);
        let length = piece.len();
        reader.consume(length);
    }
    Ok(faster_hex::hex_string(&hasher.finalize()))
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
    use std::fs;

    use super::{is_hex, sha256_file, sha256_hex};

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
    fn the_digest_of_a_file_is_the_digest_of_its_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let small = dir.path().join("abc");
        fs::write(&small, "abc").unwrap();
        assert_eq!(sha256_file(&small).unwrap(), sha256_hex(b"abc"));

        // Larger than the buffer a read is made in, and not a multiple of it.
        let bytes: Vec<u8> = (0..300_007_u32)
            .map(|index| u8::try_from(index % 251).unwrap())
            .collect();
        let large = dir.path().join("large");
        fs::write(&large, &bytes).unwrap();
        assert_eq!(sha256_file(&large).unwrap(), sha256_hex(&bytes));
    }

    #[test]
    fn a_file_that_is_not_there_has_no_digest() {
        let dir = tempfile::tempdir().unwrap();
        let error = sha256_file(&dir.path().join("nowhere"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("nowhere"), "{error}");
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

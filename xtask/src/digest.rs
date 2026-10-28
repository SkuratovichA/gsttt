//! Digests as text: the lowercase hexadecimal form of the checksums that `dist` writes and `bench` verifies.

/// The lowercase hexadecimal digits of `bytes`.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0x0f])
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::hex;

    #[test]
    fn hex_writes_two_lowercase_digits_per_byte() {
        assert_eq!(hex(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
        assert_eq!(hex(&[]), "");
    }
}

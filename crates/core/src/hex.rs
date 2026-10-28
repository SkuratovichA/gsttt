//! Bytes as lower-case hexadecimal, the one form the ids of files and the trust keys are written in.

use std::fmt::Write as _;

/// `bytes` as two lower-case hex digits for each byte, in order.
pub fn encode(bytes: impl AsRef<[u8]>) -> String {
    let bytes = bytes.as_ref();
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // Writing to a String cannot fail.
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn every_byte_is_two_lower_case_digits_with_a_leading_zero() {
        assert_eq!(encode([0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
        assert_eq!(encode([0xab, 0xcd, 0xef]), "abcdef");
    }

    #[test]
    fn no_bytes_are_no_digits() {
        assert_eq!(encode([]), "");
    }

    #[test]
    fn a_slice_an_array_and_a_vector_give_the_same_digits() {
        assert_eq!(encode([1_u8, 2, 254]), "0102fe");
        assert_eq!(encode([1_u8, 2, 254].as_slice()), "0102fe");
        assert_eq!(encode(vec![1_u8, 2, 254]), "0102fe");
    }

    proptest! {
        #[test]
        fn the_digits_read_back_as_the_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..64)) {
            let hex = encode(&bytes);

            prop_assert_eq!(hex.len(), bytes.len() * 2);
            prop_assert!(hex.bytes().all(|digit| digit.is_ascii_digit() || (b'a'..=b'f').contains(&digit)));
            let back: Vec<u8> = hex
                .as_bytes()
                .chunks(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect();
            prop_assert_eq!(back, bytes);
        }
    }
}

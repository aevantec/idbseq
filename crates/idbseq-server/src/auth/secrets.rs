//! Random tokens and hashing for credentials.

use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// 256 random bits as 64 lowercase hex characters.
#[must_use]
pub fn random_token() -> String {
    let bytes: [u8; 32] = rand::random();
    hex::encode(bytes)
}

/// A short random identifier (48 bits, 12 hex characters).
#[must_use]
pub fn random_id() -> String {
    let bytes: [u8; 6] = rand::random();
    hex::encode(bytes)
}

/// SHA-256 of `secret` as hex. Secrets are 256-bit random, so a fast hash is enough.
#[must_use]
pub fn sha256_hex(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}

/// Compare two strings in time independent of where they differ.
#[must_use]
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_256_bit_hex_and_unique() {
        let (a, b) = (random_token(), random_token());

        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
        assert_eq!(random_id().len(), 12);
    }

    #[test]
    fn hashes_and_compares() {
        let hash = sha256_hex("secret");

        assert_eq!(hash.len(), 64);
        assert!(constant_time_eq(&hash, &sha256_hex("secret")));
        assert!(!constant_time_eq(&hash, &sha256_hex("other")));
        assert!(!constant_time_eq("short", "longer"));
    }
}

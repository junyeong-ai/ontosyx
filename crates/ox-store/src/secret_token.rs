//! Cryptographically random hex-encoded tokens.
//!
//! Used for share tokens (dashboards, in the future report links) and
//! API key plaintexts. Centralised so every secret on the wire goes
//! through the same `OsRng` source — no UUID-concatenation tricks, no
//! `rand::thread_rng()` weak-seed risk.

use rand::RngCore;

/// Generate a hex-encoded token containing `bytes` bytes of CSPRNG
/// material (so the returned string is `2 * bytes` characters long).
///
/// 32 bytes = 256 bits of entropy = effectively unguessable. Caller
/// stores only the hash (e.g. via `secret_hash_sha256`); the plaintext
/// is shown to the operator exactly once.
pub fn generate_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    use std::fmt::Write;
    buf.iter()
        .fold(String::with_capacity(bytes * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// SHA-256 digest of the given bytes. Used to convert a plaintext
/// secret into the form persisted in `api_keys.key_hash`.
pub fn secret_hash_sha256(bytes: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_hex_returns_correct_length() {
        assert_eq!(generate_hex(16).len(), 32);
        assert_eq!(generate_hex(32).len(), 64);
        assert_eq!(generate_hex(64).len(), 128);
    }

    #[test]
    fn generate_hex_is_unique_across_calls() {
        let a = generate_hex(32);
        let b = generate_hex(32);
        assert_ne!(a, b, "two CSPRNG calls collided — RNG broken");
    }

    #[test]
    fn generate_hex_only_uses_lowercase_hex_chars() {
        let token = generate_hex(32);
        assert!(
            token
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "token contains non-hex char: {token}"
        );
    }

    #[test]
    fn secret_hash_sha256_matches_known_vector() {
        // SHA-256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
        let h = secret_hash_sha256(b"hello");
        assert_eq!(
            hex_lower(&h),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    fn hex_lower(bytes: &[u8]) -> String {
        use std::fmt::Write;
        bytes.iter().fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
    }
}

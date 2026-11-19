//! Passphrase cryptography for backups (spec D17) and the app lock (D14).
//!
//! Backups: Argon2id derives a 256-bit key from the passphrase; AES-256-GCM
//! encrypts the database snapshot. The plaintext header is authenticated as
//! associated data, so editing it makes the file fail to open.
//!
//! App lock: the PIN and the recovery code are stored only as Argon2id
//! hashes with their own salts.

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Argon2id cost. Stored with every hash so it can be raised later without
/// breaking existing backups or PINs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    /// Memory in KiB.
    pub m_kib: u32,
    /// Iterations.
    pub t: u32,
    /// Lanes.
    pub p: u32,
}

impl KdfParams {
    /// 64 MiB, 3 passes — OWASP's recommended Argon2id floor is 19 MiB / 2.
    pub const STANDARD: KdfParams = KdfParams {
        m_kib: 64 * 1024,
        t: 3,
        p: 1,
    };

    fn sane(&self) -> bool {
        (8 * 1024..=1024 * 1024).contains(&self.m_kib)
            && (1..=10).contains(&self.t)
            && (1..=8).contains(&self.p)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CryptoError {
    #[error("the passphrase is wrong, or the file was changed after it was made")]
    WrongPassphrase,
    #[error("the file uses settings this version can't read")]
    Unsupported,
    #[error("no secure random numbers are available")]
    Random,
}

pub fn random_bytes<const N: usize>() -> Result<[u8; N], CryptoError> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|_| CryptoError::Random)?;
    Ok(b)
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn from_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Argon2id(secret, salt) → 32 bytes.
pub fn derive_key(
    secret: &[u8],
    salt: &[u8],
    params: KdfParams,
) -> Result<Zeroizing<[u8; 32]>, CryptoError> {
    if !params.sane() || salt.len() < 16 {
        return Err(CryptoError::Unsupported);
    }
    let p = Params::new(params.m_kib, params.t, params.p, Some(32))
        .map_err(|_| CryptoError::Unsupported)?;
    let mut out = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, p)
        .hash_password_into(secret, salt, out.as_mut())
        .map_err(|_| CryptoError::Unsupported)?;
    Ok(out)
}

/// AES-256-GCM encrypt with `aad` bound to the ciphertext.
pub fn seal(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    cipher
        .encrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .expect("AES-GCM encryption of an in-memory buffer cannot fail")
}

pub fn open(
    key: &[u8; 32],
    nonce: &[u8],
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    if nonce.len() != 12 {
        return Err(CryptoError::Unsupported);
    }
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| CryptoError::WrongPassphrase)
}

/// A salted Argon2id hash of a short secret (PIN, recovery code).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretHash {
    pub kdf: KdfParams,
    pub salt: String,
    pub hash: String,
}

impl SecretHash {
    pub fn new(secret: &str, kdf: KdfParams) -> Result<Self, CryptoError> {
        let salt = random_bytes::<16>()?;
        let key = derive_key(secret.as_bytes(), &salt, kdf)?;
        Ok(SecretHash {
            kdf,
            salt: to_hex(&salt),
            hash: to_hex(key.as_ref()),
        })
    }

    /// Constant-time comparison of the derived hash.
    pub fn verify(&self, secret: &str) -> bool {
        let (Some(salt), Some(want)) = (from_hex(&self.salt), from_hex(&self.hash)) else {
            return false;
        };
        let Ok(got) = derive_key(secret.as_bytes(), &salt, self.kdf) else {
            return false;
        };
        want.len() == 32
            && got
                .iter()
                .zip(want.iter())
                .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                == 0
    }
}

#[cfg(test)]
pub(crate) const TEST_KDF: KdfParams = KdfParams {
    m_kib: 8 * 1024,
    t: 1,
    p: 1,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_and_open_round_trip_and_detect_tampering() {
        let salt = random_bytes::<16>().unwrap();
        let key = derive_key(b"correct horse", &salt, TEST_KDF).unwrap();
        let nonce = random_bytes::<12>().unwrap();
        let ct = seal(&key, &nonce, b"header", b"ledger");
        assert_eq!(
            open(&key, &nonce, b"header", &ct).unwrap().as_slice(),
            b"ledger"
        );
        assert_eq!(
            open(&key, &nonce, b"HEADER", &ct).unwrap_err(),
            CryptoError::WrongPassphrase
        );
        let wrong = derive_key(b"wrong horse", &salt, TEST_KDF).unwrap();
        assert_eq!(
            open(&wrong, &nonce, b"header", &ct).unwrap_err(),
            CryptoError::WrongPassphrase
        );
        let mut bad = ct.clone();
        bad[0] ^= 1;
        assert!(open(&key, &nonce, b"header", &bad).is_err());
    }

    #[test]
    fn secret_hashes_verify_only_the_secret() {
        let h = SecretHash::new("4821", TEST_KDF).unwrap();
        assert!(h.verify("4821"));
        assert!(!h.verify("4822"));
        assert!(!h.verify(""));
        assert_ne!(SecretHash::new("4821", TEST_KDF).unwrap().salt, h.salt);
    }

    #[test]
    fn refuses_unreasonable_parameters() {
        let p = KdfParams {
            m_kib: 10,
            t: 1,
            p: 1,
        };
        assert_eq!(
            derive_key(b"x", &[0u8; 16], p).unwrap_err(),
            CryptoError::Unsupported
        );
        assert_eq!(from_hex("zz"), None);
        assert_eq!(from_hex("0aff"), Some(vec![10, 255]));
    }
}

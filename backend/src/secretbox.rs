//! Authenticated encryption for secrets that must survive in the database but
//! must not be readable from a database leak.
//!
//! Only the TOTP secret uses this today. A TOTP secret is a bearer credential
//! for the second factor: whoever holds it can generate valid codes forever, so
//! a dump of the users table would hand over 2FA alongside the password hashes
//! it is meant to back up.
//!
//! The key lives in `TOTP_ENCRYPTION_KEY` and is separate from `JWT_SECRET` on
//! purpose. Losing it permanently destroys every enrolled second factor.

use chacha20poly1305::{
    ChaCha20Poly1305, Key, Nonce,
    aead::{Aead, KeyInit},
};
use rand::RngCore;

/// Prefix on every stored value, so a later key rotation or cipher change is
/// detectable rather than guessed at.
const VERSION: &str = "v1";
const NONCE_BYTES: usize = 12;
const KEY_BYTES: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error(
        "TOTP_ENCRYPTION_KEY must be {KEY_BYTES} bytes as {} hex characters, \
         generate one with: openssl rand -hex {KEY_BYTES}",
        KEY_BYTES * 2
    )]
    Malformed,
    #[error(
        "TOTP_ENCRYPTION_KEY must not be the same value as JWT_SECRET, \
         because a single leaked secret would then expose both sessions and second factors"
    )]
    SameAsJwtSecret,
}

#[derive(Debug, thiserror::Error)]
pub enum SealError {
    #[error("could not encrypt secret")]
    Seal,
    #[error("could not decrypt secret: wrong key, or the stored value is damaged")]
    Open,
}

pub struct SecretBox {
    cipher: ChaCha20Poly1305,
}

impl SecretBox {
    /// Reads the key from the environment.
    ///
    /// `Ok(None)` means no key is configured, which is a valid state for a
    /// deployment with no enrolled second factors. The caller decides whether
    /// that is acceptable given what is already stored.
    pub fn from_env(jwt_secret: &str) -> Result<Option<Self>, KeyError> {
        let raw = match std::env::var("TOTP_ENCRYPTION_KEY") {
            Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => return Ok(None),
        };
        if raw == jwt_secret {
            return Err(KeyError::SameAsJwtSecret);
        }
        let bytes = hex::decode(&raw).map_err(|_| KeyError::Malformed)?;
        if bytes.len() != KEY_BYTES {
            return Err(KeyError::Malformed);
        }
        let key = Key::from_slice(&bytes);
        Ok(Some(Self {
            cipher: ChaCha20Poly1305::new(key),
        }))
    }

    /// Builds a box from raw key bytes. For tests.
    #[cfg(test)]
    pub fn from_bytes(bytes: &[u8; KEY_BYTES]) -> Self {
        Self {
            cipher: ChaCha20Poly1305::new(Key::from_slice(bytes)),
        }
    }

    /// Encrypts to `v1:{nonce}:{ciphertext}`, both hex. A fresh random nonce
    /// per call, so the same secret never encrypts to the same string twice.
    pub fn seal(&self, plaintext: &str) -> Result<String, SealError> {
        let mut nonce_bytes = [0u8; NONCE_BYTES];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|_| SealError::Seal)?;
        Ok(format!(
            "{VERSION}:{}:{}",
            hex::encode(nonce_bytes),
            hex::encode(ciphertext)
        ))
    }

    pub fn open(&self, stored: &str) -> Result<String, SealError> {
        let mut parts = stored.splitn(3, ':');
        let (Some(VERSION), Some(nonce_hex), Some(ct_hex)) =
            (parts.next(), parts.next(), parts.next())
        else {
            return Err(SealError::Open);
        };
        let nonce_bytes = hex::decode(nonce_hex).map_err(|_| SealError::Open)?;
        if nonce_bytes.len() != NONCE_BYTES {
            return Err(SealError::Open);
        }
        let ciphertext = hex::decode(ct_hex).map_err(|_| SealError::Open)?;
        let plaintext = self
            .cipher
            .decrypt(Nonce::from_slice(&nonce_bytes), ciphertext.as_ref())
            .map_err(|_| SealError::Open)?;
        String::from_utf8(plaintext).map_err(|_| SealError::Open)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_with(seed: u8) -> SecretBox {
        SecretBox::from_bytes(&[seed; KEY_BYTES])
    }

    #[test]
    fn a_sealed_secret_opens_to_the_original() {
        let sb = box_with(1);
        let sealed = sb.seal("JBSWY3DPEHPK3PXP").expect("seal");
        assert!(sealed.starts_with("v1:"));
        assert!(
            !sealed.contains("JBSWY3DPEHPK3PXP"),
            "plaintext must not survive"
        );
        assert_eq!(sb.open(&sealed).expect("open"), "JBSWY3DPEHPK3PXP");
    }

    /// A fresh nonce per call, so two enrolments of the same secret do not
    /// produce the same stored value and cannot be matched against each other.
    #[test]
    fn sealing_twice_gives_different_ciphertext() {
        let sb = box_with(2);
        let a = sb.seal("SAMESECRET").expect("seal");
        let b = sb.seal("SAMESECRET").expect("seal");
        assert_ne!(a, b);
        assert_eq!(sb.open(&a).expect("open"), sb.open(&b).expect("open"));
    }

    /// The wrong key must fail loudly rather than return rubbish. This is what
    /// the startup self check relies on.
    #[test]
    fn the_wrong_key_cannot_open_a_secret() {
        let sealed = box_with(3).seal("JBSWY3DPEHPK3PXP").expect("seal");
        assert!(box_with(4).open(&sealed).is_err());
    }

    /// Poly1305 authenticates the ciphertext, so a flipped byte is rejected
    /// rather than decrypted into something else.
    #[test]
    fn a_tampered_value_is_rejected() {
        let sb = box_with(5);
        let sealed = sb.seal("JBSWY3DPEHPK3PXP").expect("seal");
        let (head, tail) = sealed.split_at(sealed.len() - 1);
        let flipped = format!("{head}{}", if tail == "0" { "1" } else { "0" });
        assert!(sb.open(&flipped).is_err());

        for junk in [
            "",
            "v1",
            "v1::",
            "v2:aa:bb",
            "not-encrypted-at-all",
            "v1:zz:zz",
        ] {
            assert!(sb.open(junk).is_err(), "{junk:?} must not open");
        }
    }

    #[test]
    fn the_key_must_differ_from_the_jwt_secret() {
        let shared = "a".repeat(64);
        // SAFETY: single threaded test.
        unsafe { std::env::set_var("TOTP_ENCRYPTION_KEY", &shared) };
        assert!(matches!(
            SecretBox::from_env(&shared),
            Err(KeyError::SameAsJwtSecret)
        ));

        unsafe { std::env::set_var("TOTP_ENCRYPTION_KEY", "not-hex") };
        assert!(matches!(
            SecretBox::from_env("jwt"),
            Err(KeyError::Malformed)
        ));

        unsafe { std::env::set_var("TOTP_ENCRYPTION_KEY", "abcd") };
        assert!(matches!(
            SecretBox::from_env("jwt"),
            Err(KeyError::Malformed)
        ));

        unsafe { std::env::remove_var("TOTP_ENCRYPTION_KEY") };
        assert!(
            SecretBox::from_env("jwt")
                .expect("unset is allowed")
                .is_none()
        );
    }
}

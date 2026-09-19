use crate::error::{Error, Result};
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use rand::{RngCore, rngs::OsRng};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};

pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
pub fn hash(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
pub fn sign<T: Serialize>(value: &T, secret: &[u8]) -> Result<String> {
    let payload = serde_json::to_vec(value).map_err(|e| Error::Internal(e.to_string()))?;
    let body = URL_SAFE_NO_PAD.encode(payload);
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret).map_err(|e| Error::Internal(e.to_string()))?;
    mac.update(body.as_bytes());
    Ok(format!(
        "{body}.{}",
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    ))
}
pub fn verify<T: DeserializeOwned>(token: &str, secret: &[u8]) -> Result<T> {
    if token.len() > 4096 {
        return Err(Error::Unauthorized);
    }
    let (body, signature) = token.split_once('.').ok_or(Error::Unauthorized)?;
    let signature = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| Error::Unauthorized)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).map_err(|_| Error::Unauthorized)?;
    mac.update(body.as_bytes());
    mac.verify_slice(&signature)
        .map_err(|_| Error::Unauthorized)?;
    serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(body)
            .map_err(|_| Error::Unauthorized)?,
    )
    .map_err(|_| Error::Unauthorized)
}
pub fn valid_secret(value: &str) -> bool {
    value.len() >= 32
}
pub fn verify_admin(value: &str, configured: &str) -> bool {
    // HMAC verification performs a constant-time comparison of fixed-size tags.
    let mut mac =
        Hmac::<Sha256>::new_from_slice(configured.as_bytes()).expect("any HMAC key length");
    mac.update(b"polyntu-admin");
    let mut candidate =
        Hmac::<Sha256>::new_from_slice(value.as_bytes()).expect("any HMAC key length");
    candidate.update(b"polyntu-admin");
    mac.verify_slice(&candidate.finalize().into_bytes()).is_ok()
}

/// NTU-affiliated addresses only: `name@ntu.edu.sg` or
/// `name@unit.ntu.edu.sg` (ADR 0005). Callers normalize to lowercase first;
/// uppercase domains are rejected so nothing bypasses the unique constraint.
pub fn valid_ntu_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    if local.is_empty()
        || local.len() > 64
        || email.len() > 254
        || email.contains(char::is_whitespace)
        || domain.contains('@')
    {
        return false;
    }
    let labels: Vec<&str> = domain.split('.').collect();
    labels.len() >= 3
        && labels[labels.len() - 3..] == ["ntu", "edu", "sg"]
        && labels.iter().all(|label| {
            !label.is_empty()
                && label
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

/// argon2id hash in PHC string form with a fresh random salt. Registration
/// is not a hot path, so the default cost parameters apply.
pub fn hash_password(password: &str) -> Result<String> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|e| Error::Internal(e.to_string()))
}

/// A malformed stored hash fails closed instead of erroring, so callers can
/// treat every verification the same way.
pub fn verify_password(stored: &str, password: &str) -> bool {
    PasswordHash::new(stored)
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ntu_addresses_only() {
        for valid in [
            "billy@ntu.edu.sg",
            "billy@scse.ntu.edu.sg",
            "b-2.x@cce.ntu.edu.sg",
        ] {
            assert!(valid_ntu_email(valid), "should accept {valid}");
        }
        for invalid in [
            "",
            "billy@gmail.com",
            "billy@ntu.edu",
            "@ntu.edu.sg",
            "billy@",
            "billy@@ntu.edu.sg",
            "bil ly@ntu.edu.sg",
            "billy@xntu.edu.sg",
            "billy@ntu.edu.sg.evil.com",
            "billy@.ntu.edu.sg",
            "billy@NTU.edu.sg",
        ] {
            assert!(!valid_ntu_email(invalid), "should reject {invalid}");
        }
    }

    #[test]
    fn password_hashes_round_trip() {
        let hash = hash_password("correct horse battery").unwrap();
        assert!(verify_password(&hash, "correct horse battery"));
        assert!(!verify_password(&hash, "a different password"));
        assert!(!verify_password("not-a-phc-hash", "correct horse battery"));
    }
}

use crate::error::{Error, Result};
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use rand::{TryRng, rngs::SysRng};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};

pub fn random_token() -> Result<String> {
    let mut bytes = [0u8; 32];
    SysRng
        .try_fill_bytes(&mut bytes)
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
pub fn hash(value: &[u8]) -> String {
    // sha2 0.11 no longer formats digests as hex, and the stored token hashes
    // must keep their exact historical encoding.
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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

/// The exact bytes a creator signs for a human resolution (ADR 0007).
pub fn resolution_message(instance_id: &str, outcome_id: &str, nonce: &str) -> String {
    format!("polyntu.resolution.v1:{instance_id}:{outcome_id}:{nonce}")
}

/// Verify a creator's ed25519 signature over the resolution message. The
/// platform holds only the public key, so it can check but never forge a
/// resolution. Malformed inputs fail closed.
pub fn verify_ed25519(public_key_b64: &str, message: &str, signature_b64: &str) -> bool {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    let decode = |value: &str| {
        base64::engine::general_purpose::STANDARD
            .decode(value.trim())
            .ok()
    };
    let (Some(key_bytes), Some(signature_bytes)) = (decode(public_key_b64), decode(signature_b64))
    else {
        return false;
    };
    let key_bytes: [u8; 32] = match key_bytes.try_into() {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let signature_bytes: [u8; 64] = match signature_bytes.try_into() {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    VerifyingKey::from_bytes(&key_bytes)
        .map(|key| {
            key.verify(message.as_bytes(), &Signature::from_bytes(&signature_bytes))
                .is_ok()
        })
        .unwrap_or(false)
}

/// PBKDF2 iterations for deriving a creator's resolution signing key from the
/// account password (ADR 0007 amendment).
pub const RESOLUTION_KEY_ITERATIONS: u32 = 600_000;

/// The ed25519 seed of a creator's resolution signing key, derived from the
/// account password so that holding the password is holding the key:
/// PBKDF2-HMAC-SHA256 over the password with the email-bound salt
/// `polyntu.resolution.v1:{email}`. The creator's browser derives the same
/// seed and publishes only the resulting public key; any browser where the
/// creator signs in can re-derive it, so a lost profile no longer voids the
/// market.
pub fn resolution_key_seed(email: &str, password: &str) -> [u8; 32] {
    let salt = format!("polyntu.resolution.v1:{email}");
    let mut seed = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(
        password.as_bytes(),
        salt.as_bytes(),
        RESOLUTION_KEY_ITERATIONS,
        &mut seed,
    );
    seed
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

    #[test]
    fn resolution_key_seed_matches_the_browser_derivation() {
        // Vector produced with the browser's WebCrypto (PBKDF2-HMAC-SHA256,
        // then the ed25519 public key of the seed); the frontend derives the
        // identical keypair, and a wrong password yields a different seed.
        let seed = resolution_key_seed("billy@ntu.edu.sg", "correct horse battery");
        assert_eq!(
            seed.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "5a95d8a2f1494cd9a0d96e89f381c4bd8908cb95a635f74365d34ed868ea7e03"
        );
        let signing = ed25519_dalek::SigningKey::from_bytes(&seed);
        assert_eq!(
            base64::engine::general_purpose::STANDARD.encode(signing.verifying_key().as_bytes()),
            "1qpCGijHNjhfbZlvnZ4UQvlsPpvhhuhbvon+1dWrre8="
        );
        assert_ne!(
            resolution_key_seed("billy@ntu.edu.sg", "wrong password"),
            seed
        );
        assert_ne!(
            resolution_key_seed("other@ntu.edu.sg", "correct horse battery"),
            seed
        );
    }
}

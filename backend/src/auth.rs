use crate::error::{Error, Result};
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

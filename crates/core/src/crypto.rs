use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, Context, Result};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};

/// What an inbound API key is, before any database lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeyClass {
    /// `sk-v1` whose HMAC matches. `user_id` came from the signed payload.
    Verified { user_id: i64 },
    /// Legacy `sk-` plus 48 hex chars. The account is only known after a lookup.
    Legacy,
    /// Missing, malformed, or a failed MAC. Must not spend admission quota.
    Invalid,
}

pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow!("password hash failed: {e}"))?
        .to_string();
    Ok(hash)
}

pub fn verify_password(password: &str, password_hash: &str) -> Result<bool> {
    let parsed =
        PasswordHash::new(password_hash).map_err(|e| anyhow!("invalid password hash: {e}"))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

pub fn hash_token_key(raw_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_key.as_bytes());
    hex::encode(hasher.finalize())
}

/// `sk-v1.{base64url(user_id_be64 || random_16)}.{base64url(hmac_16)}`.
///
/// The MAC key is `HMAC-SHA256(encryption_key, "danmo-api-key-v1")`. The MAC covers the
/// 24-byte payload and is truncated to 16 bytes, so a forged or edited key fails closed.
pub fn generate_api_key(encryption_key: &[u8; 32], user_id: i64) -> String {
    let mut random = [0u8; 16];
    OsRng.fill_bytes(&mut random);
    let mut payload = [0u8; 24];
    payload[..8].copy_from_slice(&user_id.to_be_bytes());
    payload[8..].copy_from_slice(&random);
    let mac = sign_payload(encryption_key, &payload);
    format!(
        "sk-v1.{}.{}",
        URL_SAFE_NO_PAD.encode(payload),
        URL_SAFE_NO_PAD.encode(mac)
    )
}

/// Classify a presented credential. Accepts an optional `Bearer ` prefix. Does not touch storage.
pub fn classify_api_key(encryption_key: &[u8; 32], presented: &str) -> ApiKeyClass {
    let key = presented.trim();
    let key = key.strip_prefix("Bearer ").unwrap_or(key).trim();
    if key.is_empty() {
        return ApiKeyClass::Invalid;
    }
    if let Some(rest) = key.strip_prefix("sk-v1.") {
        return classify_v1(encryption_key, rest);
    }
    if is_legacy_key(key) {
        ApiKeyClass::Legacy
    } else {
        ApiKeyClass::Invalid
    }
}

fn classify_v1(encryption_key: &[u8; 32], rest: &str) -> ApiKeyClass {
    let Some((payload_b64, mac_b64)) = rest.split_once('.') else {
        return ApiKeyClass::Invalid;
    };
    if payload_b64.is_empty() || mac_b64.is_empty() || mac_b64.contains('.') {
        return ApiKeyClass::Invalid;
    }
    let Ok(payload) = URL_SAFE_NO_PAD.decode(payload_b64) else {
        return ApiKeyClass::Invalid;
    };
    let Ok(mac) = URL_SAFE_NO_PAD.decode(mac_b64) else {
        return ApiKeyClass::Invalid;
    };
    if payload.len() != 24 || mac.len() != 16 {
        return ApiKeyClass::Invalid;
    }
    let mut payload_arr = [0u8; 24];
    payload_arr.copy_from_slice(&payload);
    let expected = sign_payload(encryption_key, &payload_arr);
    if !constant_time_eq(&expected, &mac) {
        return ApiKeyClass::Invalid;
    }
    let mut id_bytes = [0u8; 8];
    id_bytes.copy_from_slice(&payload_arr[..8]);
    ApiKeyClass::Verified {
        user_id: i64::from_be_bytes(id_bytes),
    }
}

fn is_legacy_key(key: &str) -> bool {
    let Some(rest) = key.strip_prefix("sk-") else {
        return false;
    };
    rest.len() == 48 && rest.bytes().all(|b| b.is_ascii_hexdigit())
}

fn sign_payload(encryption_key: &[u8; 32], payload: &[u8; 24]) -> [u8; 16] {
    let mac_key = hmac_sha256(encryption_key, b"danmo-api-key-v1");
    let mac = hmac_sha256(&mac_key, payload);
    let mut short = [0u8; 16];
    short.copy_from_slice(&mac[..16]);
    short
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut key_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        let hashed = Sha256::digest(key);
        key_block[..hashed.len()].copy_from_slice(&hashed);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut i_key = [0u8; BLOCK];
    let mut o_key = [0u8; BLOCK];
    for i in 0..BLOCK {
        i_key[i] = key_block[i] ^ 0x36;
        o_key[i] = key_block[i] ^ 0x5c;
    }
    let mut inner = Sha256::new();
    inner.update(i_key);
    inner.update(message);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(o_key);
    outer.update(inner_hash);
    let outer_hash = outer.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&outer_hash);
    out
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn encrypt_secret(key: &[u8; 32], plaintext: &str) -> Result<String> {
    let cipher = Aes256Gcm::new_from_slice(key).context("invalid encryption key")?;
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow!("encrypt failed: {e}"))?;
    let mut out = nonce_bytes.to_vec();
    out.extend(ciphertext);
    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        out,
    ))
}

pub fn decrypt_secret(key: &[u8; 32], encoded: &str) -> Result<String> {
    let raw = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
        .context("invalid ciphertext encoding")?;
    if raw.len() < 12 {
        return Err(anyhow!("ciphertext too short"));
    }
    let (nonce_bytes, ciphertext) = raw.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(key).context("invalid encryption key")?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow!("decrypt failed: {e}"))?;
    String::from_utf8(plaintext).context("decrypted secret is not utf8")
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    #[test]
    fn password_roundtrip() {
        let hash = hash_password("secret").unwrap();
        assert!(verify_password("secret", &hash).unwrap());
        assert!(!verify_password("wrong", &hash).unwrap());
    }

    #[test]
    fn secret_roundtrip() {
        let key = [7u8; 32];
        let enc = encrypt_secret(&key, "sk-upstream").unwrap();
        assert_eq!(decrypt_secret(&key, &enc).unwrap(), "sk-upstream");
    }

    #[test]
    fn hmac_sha256_matches_rfc4231_case1() {
        let key = [0x0bu8; 20];
        let mac = hmac_sha256(&key, b"Hi There");
        assert_eq!(
            hex::encode(mac),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn v1_key_roundtrip_and_rejects_edits() {
        let key = [9u8; 32];
        let raw = generate_api_key(&key, 42);
        assert!(raw.starts_with("sk-v1."));
        assert_eq!(
            classify_api_key(&key, &format!("Bearer {raw}")),
            ApiKeyClass::Verified { user_id: 42 }
        );

        let (payload, mac) = raw.strip_prefix("sk-v1.").unwrap().split_once('.').unwrap();
        let mut payload_bytes = URL_SAFE_NO_PAD.decode(payload).unwrap();
        payload_bytes[7] ^= 0x01;
        let edited_id = format!("sk-v1.{}.{}", URL_SAFE_NO_PAD.encode(&payload_bytes), mac);
        assert_eq!(classify_api_key(&key, &edited_id), ApiKeyClass::Invalid);

        let mut random_edit = URL_SAFE_NO_PAD.decode(payload).unwrap();
        random_edit[8] ^= 0xff;
        let edited_random = format!("sk-v1.{}.{}", URL_SAFE_NO_PAD.encode(&random_edit), mac);
        assert_eq!(classify_api_key(&key, &edited_random), ApiKeyClass::Invalid);

        let mut mac_bytes = URL_SAFE_NO_PAD.decode(mac).unwrap();
        mac_bytes[0] ^= 0x01;
        let edited_mac = format!("sk-v1.{}.{}", payload, URL_SAFE_NO_PAD.encode(&mac_bytes));
        assert_eq!(classify_api_key(&key, &edited_mac), ApiKeyClass::Invalid);

        assert_eq!(classify_api_key(&key, ""), ApiKeyClass::Invalid);
        assert_eq!(
            classify_api_key(&key, "sk-v1.not-a-key"),
            ApiKeyClass::Invalid
        );
        let legacy = format!("sk-{}", "ab".repeat(24));
        assert_eq!(classify_api_key(&key, &legacy), ApiKeyClass::Legacy);
        assert_eq!(classify_api_key(&[3u8; 32], &raw), ApiKeyClass::Invalid);
    }
}

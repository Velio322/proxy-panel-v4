use crate::error::{AppError, AppResult};
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine as _;
use rand::rngs::OsRng;
use rand::{Rng, RngCore};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

pub const AAD_PROTOCOL_CREDENTIAL: &str = "proxpanel:protocol-credential:v1";
pub const AAD_CONFIG_TRANSFER_V2: &str = "proxpanel:config-transfer:v2";

pub const MAX_PROTOCOL_CREDENTIAL_LEN: usize = 4096;
pub const MAX_TRANSFER_SECRET_LEN: usize = 1024 * 1024;

/// Constant-time string comparison using `subtle::ConstantTimeEq`.
pub fn ct_eq(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    if ab.len() != bb.len() {
        return false;
    }
    ab.ct_eq(bb).into()
}

/// Computes lowercase hex SHA-256 digest of `input`.
pub fn sha256_hex(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    hex::encode(digest)
}

/// Computes lowercase hex SHA-256 digest of raw bytes.
pub fn sha256_bytes_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(digest)
}

/// Parses a 32-byte AES-256 key from either 64 hex chars or base64 (standard/url-safe, padded/unpadded).
pub fn parse_encryption_key(raw: &str) -> AppResult<[u8; 32]> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(AppError::bad_request(
            "ENCRYPTION_KEY must contain 32 bytes encoded as hex or base64",
        ));
    }
    let decoded = if s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        hex::decode(s).map_err(|_| {
            AppError::bad_request("ENCRYPTION_KEY must contain 32 bytes encoded as hex or base64")
        })?
    } else {
        STANDARD
            .decode(s)
            .or_else(|_| URL_SAFE_NO_PAD.decode(s))
            .or_else(|_| URL_SAFE.decode(s))
            .or_else(|_| STANDARD_NO_PAD.decode(s))
            .map_err(|_| {
                AppError::bad_request(
                    "ENCRYPTION_KEY must contain 32 bytes encoded as hex or base64",
                )
            })?
    };

    if decoded.len() != 32 {
        return Err(AppError::bad_request(
            "ENCRYPTION_KEY must contain 32 bytes encoded as hex or base64",
        ));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&decoded);
    Ok(key)
}

/// Encrypts UTF-8 string into `v1.<b64url(iv)>.<b64url(tag)>.<b64url(ciphertext)>`.
pub fn encrypt_str(key_opt: Option<&[u8; 32]>, plaintext: &str, aad: &str) -> AppResult<String> {
    let Some(key) = key_opt else {
        return Err(AppError::bad_request(
            "ENCRYPTION_KEY must contain 32 bytes encoded as hex or base64",
        ));
    };
    let max_len = if aad == AAD_CONFIG_TRANSFER_V2 {
        MAX_TRANSFER_SECRET_LEN
    } else {
        MAX_PROTOCOL_CREDENTIAL_LEN
    };
    let pt_bytes = plaintext.as_bytes();
    if pt_bytes.is_empty() || pt_bytes.len() > max_len {
        return Err(AppError::bad_request("Secret exceeds permitted size"));
    }

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| AppError::internal("AES-256-GCM init failed"))?;
    let mut iv = [0u8; 12];
    OsRng.fill_bytes(&mut iv);

    let ct_and_tag = cipher
        .encrypt(
            Nonce::from_slice(&iv),
            Payload {
                msg: pt_bytes,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| AppError::internal("AES-256-GCM encryption failed"))?;

    if ct_and_tag.len() < 16 {
        return Err(AppError::internal("Invalid AES-GCM output"));
    }
    let split_idx = ct_and_tag.len() - 16;
    let ciphertext = &ct_and_tag[..split_idx];
    let tag = &ct_and_tag[split_idx..];

    Ok(format!(
        "v1.{}.{}.{}",
        URL_SAFE_NO_PAD.encode(iv),
        URL_SAFE_NO_PAD.encode(tag),
        URL_SAFE_NO_PAD.encode(ciphertext)
    ))
}

/// Decrypts `v1.<b64url(iv)>.<b64url(tag)>.<b64url(ciphertext)>` using AES-256-GCM with `aad`.
pub fn decrypt_str(key_opt: Option<&[u8; 32]>, encrypted: &str, aad: &str) -> AppResult<String> {
    let Some(key) = key_opt else {
        return Err(AppError::bad_request(
            "ENCRYPTION_KEY must contain 32 bytes encoded as hex or base64",
        ));
    };
    let max_len = if aad == AAD_CONFIG_TRANSFER_V2 {
        MAX_TRANSFER_SECRET_LEN
    } else {
        MAX_PROTOCOL_CREDENTIAL_LEN
    };
    let max_enc_len = (max_len * 4) / 3 + 128;
    if encrypted.is_empty() || encrypted.len() > max_enc_len {
        return Err(AppError::bad_request("Invalid encrypted secret"));
    }

    let parts: Vec<&str> = encrypted.split('.').collect();
    if parts.len() != 4 || parts[0] != "v1" {
        return Err(AppError::bad_request("Invalid encrypted secret"));
    }

    let iv = URL_SAFE_NO_PAD
        .decode(parts[1])
        .or_else(|_| URL_SAFE.decode(parts[1]))
        .map_err(|_| AppError::bad_request("Invalid encrypted protocol credential"))?;
    let tag = URL_SAFE_NO_PAD
        .decode(parts[2])
        .or_else(|_| URL_SAFE.decode(parts[2]))
        .map_err(|_| AppError::bad_request("Invalid encrypted protocol credential"))?;
    let ciphertext = URL_SAFE_NO_PAD
        .decode(parts[3])
        .or_else(|_| URL_SAFE.decode(parts[3]))
        .map_err(|_| AppError::bad_request("Invalid encrypted protocol credential"))?;

    if iv.len() != 12 || tag.len() != 16 || ciphertext.is_empty() || ciphertext.len() > max_len {
        return Err(AppError::bad_request(
            "Invalid encrypted protocol credential",
        ));
    }

    let mut combined = Vec::with_capacity(ciphertext.len() + tag.len());
    combined.extend_from_slice(&ciphertext);
    combined.extend_from_slice(&tag);

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| AppError::internal("AES-256-GCM init failed"))?;
    let pt_bytes = cipher
        .decrypt(
            Nonce::from_slice(&iv),
            Payload {
                msg: &combined,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| AppError::bad_request("Invalid encrypted protocol credential"))?;

    String::from_utf8(pt_bytes)
        .map_err(|_| AppError::bad_request("Invalid encrypted protocol credential"))
}

/// Generates a random 24-byte Base64url (unpadded, 32-char) protocol secret.
pub fn random_protocol_secret() -> String {
    let mut buf = [0u8; 24];
    OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// Generates a random 32-byte standard Base64 key (used for SS-2022 256-bit and master secrets).
pub fn random_base64_32() -> String {
    let mut buf = [0u8; 32];
    OsRng.fill_bytes(&mut buf);
    STANDARD.encode(buf)
}

/// Generates a random 16-byte standard Base64 key (used for `2022-blake3-aes-128-gcm`).
pub fn random_base64_16() -> String {
    let mut buf = [0u8; 16];
    OsRng.fill_bytes(&mut buf);
    STANDARD.encode(buf)
}

/// Generates a random hex string of `byte_len` random bytes (`2 * byte_len` hex characters).
pub fn random_hex(byte_len: usize) -> String {
    let mut buf = vec![0u8; byte_len];
    OsRng.fill_bytes(&mut buf);
    hex::encode(buf)
}

/// CSPRNG human-readable password generator (16 chars from unambiguous alphanumeric alphabet).
pub fn generate_user_password(len: usize) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789";
    let mut rng = OsRng;
    (0..len)
        .map(|_| {
            let idx = rng.gen_range(0..ALPHABET.len());
            ALPHABET[idx] as char
        })
        .collect()
}

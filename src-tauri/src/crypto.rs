/// Grimoire cryptographic primitives.
///
/// All encryption uses AES-256-GCM (authenticated, so we detect tampering).
/// Keys are derived from passwords using Argon2id (memory-hard, recommended
/// by OWASP for password hashing/KDF as of 2024).
///
/// Storage format for ciphertext blobs (stored as base64 TEXT in SQLite):
///   [12 bytes nonce][ciphertext][16 bytes GCM auth tag]
///
/// The salt is stored separately in plaintext alongside the ciphertext — salts
/// are not secret; their job is to make identical passwords produce different keys.
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngCore;

/// A known plaintext we encrypt to verify a password guess without storing the password.
/// If we can decrypt the sentinel and get back this value, the password is correct.
const SENTINEL_PLAINTEXT: &[u8] = b"grimoire_ok";

/// Generate a cryptographically-random 32-byte salt.
pub fn generate_salt() -> [u8; 32] {
    let mut salt = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    salt
}

/// Derive a 256-bit encryption key from `password` and `salt` using Argon2id.
///
/// Parameters (OWASP recommended minimum as of 2024):
///   m = 65536 KiB (64 MiB memory), t = 2 iterations, p = 1 lane
///
/// This takes ~100ms on a modern machine — acceptable for a one-time unlock,
/// slow enough to make brute-force attacks costly.
pub fn derive_key(password: &str, salt: &[u8]) -> [u8; 32] {
    let params = Params::new(65_536, 2, 1, Some(32)).expect("valid argon2 params");
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .expect("argon2 key derivation failed");
    key
}

/// Encrypt `plaintext` with `key` using AES-256-GCM.
///
/// Returns: `nonce (12 bytes) || ciphertext || auth_tag (16 bytes)`, base64-encoded.
/// The nonce is randomly generated per call, so encrypting the same plaintext
/// twice produces different output — this is intentional and required.
pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> String {
    let mut nonce_bytes = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(&nonce_bytes);

    let mut ciphertext = cipher
        .encrypt(nonce, plaintext)
        .expect("AES-GCM encryption failed");

    // Prepend nonce to ciphertext so we have a single self-contained blob.
    let mut blob = nonce_bytes.to_vec();
    blob.append(&mut ciphertext);

    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(&blob)
}

/// Decrypt a base64 blob produced by `encrypt`.
///
/// Returns `Err` if the blob is malformed, the nonce is wrong length, or the
/// GCM auth tag doesn't match (indicating wrong key or tampered data).
pub fn decrypt(key: &[u8; 32], blob_b64: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    let blob = base64::engine::general_purpose::STANDARD
        .decode(blob_b64)
        .map_err(|e| format!("base64 decode failed: {e}"))?;

    if blob.len() < 12 {
        return Err("ciphertext blob too short".to_string());
    }

    let (nonce_bytes, ciphertext) = blob.split_at(12);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "decryption failed — wrong key or corrupted data".to_string())
}

/// Produce a sentinel blob: encrypt the known plaintext with `key`.
/// Store this alongside the salt so we can later verify a password attempt.
pub fn make_sentinel(key: &[u8; 32]) -> String {
    encrypt(key, SENTINEL_PLAINTEXT)
}

/// Return true if `key` correctly decrypts `sentinel` back to the known plaintext.
pub fn verify_sentinel(key: &[u8; 32], sentinel_b64: &str) -> bool {
    match decrypt(key, sentinel_b64) {
        Ok(plaintext) => plaintext == SENTINEL_PLAINTEXT,
        Err(_) => false,
    }
}



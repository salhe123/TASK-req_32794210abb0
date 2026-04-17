use std::fs;
use std::path::Path;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use once_cell::sync::OnceCell;
use rand::RngCore;

use crate::errors::AppError;

static KEK: OnceCell<[u8; 32]> = OnceCell::new();

pub fn init_kek(path: &str) -> anyhow::Result<()> {
    let bytes = if Path::new(path).exists() {
        fs::read(path)?
    } else {
        let mut buf = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut buf);
        if let Some(parent) = Path::new(path).parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(path, &buf)?;
        buf
    };
    if bytes.len() != 32 {
        return Err(anyhow::anyhow!("KEK must be 32 bytes"));
    }
    let mut kek = [0u8; 32];
    kek.copy_from_slice(&bytes);
    let _ = KEK.set(kek);
    Ok(())
}

/// Test helper: seed a deterministic KEK for unit tests.
#[cfg(test)]
pub fn init_kek_for_test() {
    let _ = KEK.set([7u8; 32]);
}

fn cipher() -> Result<Aes256Gcm, AppError> {
    let kek = KEK
        .get()
        .ok_or_else(|| AppError::Internal("KEK not initialized".into()))?;
    Ok(Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(kek)))
}

pub fn encrypt(plaintext: &[u8]) -> Result<Vec<u8>, AppError> {
    let c = cipher()?;
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = c
        .encrypt(nonce, plaintext)
        .map_err(|e| AppError::Internal(format!("encrypt: {}", e)))?;
    let mut out = Vec::with_capacity(12 + ct.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

pub fn decrypt(blob: &[u8]) -> Result<Vec<u8>, AppError> {
    if blob.len() < 12 {
        return Err(AppError::Internal("ciphertext too short".into()));
    }
    let c = cipher()?;
    let nonce = Nonce::from_slice(&blob[..12]);
    c.decrypt(nonce, &blob[12..])
        .map_err(|e| AppError::Internal(format!("decrypt: {}", e)))
}

/// Mask all but the last 4 characters of a string with `*`.
pub fn mask_last4(s: &str) -> String {
    let count = s.chars().count();
    if count <= 4 {
        return "*".repeat(count);
    }
    let mask_len = count - 4;
    let last4: String = s.chars().skip(mask_len).collect();
    format!("{}{}", "*".repeat(mask_len), last4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_short() {
        assert_eq!(mask_last4("abc"), "***");
    }

    #[test]
    fn mask_long() {
        assert_eq!(mask_last4("1234567890"), "******7890");
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        init_kek_for_test();
        let plain = b"hello world";
        let blob = encrypt(plain).unwrap();
        assert_ne!(blob, plain);
        let back = decrypt(&blob).unwrap();
        assert_eq!(back, plain);
    }
}

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use serde_json::json;

use crate::errors::AppError;

pub const MIN_PASSWORD_LEN: usize = 12;

pub fn validate_policy(password: &str) -> Result<(), AppError> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(AppError::Validation {
            message: format!("password must be at least {} characters", MIN_PASSWORD_LEN),
            details: json!({ "field": "password", "reason": "too_short" }),
        });
    }
    Ok(())
}

pub fn hash_password(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| AppError::Internal(format!("hash: {}", e)))?
        .to_string();
    Ok(hash)
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let parsed = match PasswordHash::new(hash) {
        Ok(p) => p,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_password() {
        assert!(validate_policy("short").is_err());
    }

    #[test]
    fn accepts_twelve_chars() {
        assert!(validate_policy("abcdefghijkl").is_ok());
    }

    #[test]
    fn hash_and_verify() {
        let pw = "correct-horse-battery";
        let h = hash_password(pw).unwrap();
        assert!(verify_password(pw, &h));
        assert!(!verify_password("wrong-password", &h));
    }
}

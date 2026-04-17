use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use serde_json::{json, Value};
use thiserror::Error;

/// Generic text returned to clients for any `Internal` error. Detailed
/// diagnostics are only emitted to the structured server log — see the
/// `impl ResponseError::error_response` below.
pub const GENERIC_INTERNAL_MESSAGE: &str = "internal server error";

#[derive(Debug, Error)]
pub enum AppError {
    #[error("validation failed")]
    Validation { message: String, details: Value },

    #[error("invalid attachment")]
    InvalidAttachment(String),

    #[error("unauthenticated")]
    Unauthenticated,

    #[error("session expired")]
    SessionExpired,

    #[error("forbidden")]
    Forbidden,

    #[error("out of scope")]
    OutOfScope,

    #[error("not found")]
    NotFound,

    #[error("invalid transition")]
    InvalidTransition(String),

    #[error("duplicate asset label")]
    DuplicateAssetLabel,

    #[error("idempotency conflict")]
    IdempotencyConflict,

    #[error("attachment limit exceeded")]
    AttachmentLimit { limit: &'static str },

    #[error("account locked")]
    AccountLocked,

    #[error("rate limited")]
    RateLimited,

    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation { .. } => "validation_failed",
            Self::InvalidAttachment(_) => "invalid_attachment",
            Self::Unauthenticated => "unauthenticated",
            Self::SessionExpired => "session_expired",
            Self::Forbidden => "forbidden",
            Self::OutOfScope => "out_of_scope",
            Self::NotFound => "not_found",
            Self::InvalidTransition(_) => "invalid_transition",
            Self::DuplicateAssetLabel => "duplicate_asset_label",
            Self::IdempotencyConflict => "idempotency_conflict",
            Self::AttachmentLimit { .. } => "attachment_limit_exceeded",
            Self::AccountLocked => "account_locked",
            Self::RateLimited => "rate_limited",
            Self::Internal(_) => "internal_error",
        }
    }

    pub fn http_status(&self) -> StatusCode {
        match self {
            Self::Validation { .. } | Self::InvalidAttachment(_) => StatusCode::BAD_REQUEST,
            Self::Unauthenticated | Self::SessionExpired => StatusCode::UNAUTHORIZED,
            Self::Forbidden | Self::OutOfScope => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::InvalidTransition(_)
            | Self::DuplicateAssetLabel
            | Self::IdempotencyConflict => StatusCode::CONFLICT,
            Self::AttachmentLimit { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::AccountLocked => StatusCode::LOCKED,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn to_envelope(&self) -> Value {
        let message = self.user_message();
        let details = self.details();
        json!({
            "error": self.code(),
            "message": message,
            "details": details,
        })
    }

    fn user_message(&self) -> String {
        match self {
            Self::Validation { message, .. } => message.clone(),
            Self::InvalidAttachment(m) => m.clone(),
            Self::Unauthenticated => "authentication required".to_string(),
            Self::SessionExpired => "session expired".to_string(),
            Self::Forbidden => "action not permitted".to_string(),
            Self::OutOfScope => "resource outside your data scope".to_string(),
            Self::NotFound => "resource not found".to_string(),
            Self::InvalidTransition(m) => m.clone(),
            Self::DuplicateAssetLabel => "asset label already exists in this facility".to_string(),
            Self::IdempotencyConflict => {
                "request id already used by another caller".to_string()
            }
            Self::AttachmentLimit { limit } => {
                format!("attachment {} limit exceeded", limit)
            }
            Self::AccountLocked => "account is locked due to failed login attempts".to_string(),
            Self::RateLimited => "too many requests".to_string(),
            // Never leak internal detail — those belong in server logs only.
            Self::Internal(_) => GENERIC_INTERNAL_MESSAGE.to_string(),
        }
    }

    fn details(&self) -> Value {
        match self {
            Self::Validation { details, .. } => details.clone(),
            Self::AttachmentLimit { limit } => json!({ "limit": limit }),
            _ => json!({}),
        }
    }
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        self.http_status()
    }

    fn error_response(&self) -> HttpResponse {
        // Log internal details server-side — the client only sees the generic message.
        if let Self::Internal(detail) = self {
            tracing::error!(error = %detail, code = self.code(), "internal error");
        }
        HttpResponse::build(self.http_status()).json(self.to_envelope())
    }
}

impl From<diesel::result::Error> for AppError {
    fn from(err: diesel::result::Error) -> Self {
        use diesel::result::{DatabaseErrorKind, Error as DE};
        match err {
            DE::NotFound => AppError::NotFound,
            DE::DatabaseError(DatabaseErrorKind::UniqueViolation, info) => {
                let c = info.constraint_name().unwrap_or("");
                if c.contains("asset_label") {
                    AppError::DuplicateAssetLabel
                } else {
                    AppError::Internal(format!("unique violation: {}", info.message()))
                }
            }
            e => AppError::Internal(format!("db error: {}", e)),
        }
    }
}

impl From<r2d2::Error> for AppError {
    fn from(err: r2d2::Error) -> Self {
        AppError::Internal(format!("db pool: {}", err))
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn envelope_shape_for_validation() {
        let e = AppError::Validation {
            message: "bad".into(),
            details: json!({ "field": "x" }),
        };
        assert_eq!(e.code(), "validation_failed");
        assert_eq!(e.http_status().as_u16(), 400);
        let env = e.to_envelope();
        assert_eq!(env["error"].as_str(), Some("validation_failed"));
        assert_eq!(env["message"].as_str(), Some("bad"));
        assert_eq!(env["details"]["field"].as_str(), Some("x"));
    }

    #[test]
    fn attachment_limit_detail_included() {
        let e = AppError::AttachmentLimit { limit: "bytes" };
        assert_eq!(e.http_status().as_u16(), 413);
        assert_eq!(e.to_envelope()["details"]["limit"].as_str(), Some("bytes"));
    }

    #[test]
    fn status_codes_across_variants() {
        assert_eq!(AppError::Unauthenticated.http_status().as_u16(), 401);
        assert_eq!(AppError::SessionExpired.http_status().as_u16(), 401);
        assert_eq!(AppError::Forbidden.http_status().as_u16(), 403);
        assert_eq!(AppError::OutOfScope.http_status().as_u16(), 403);
        assert_eq!(AppError::NotFound.http_status().as_u16(), 404);
        assert_eq!(AppError::DuplicateAssetLabel.http_status().as_u16(), 409);
        assert_eq!(AppError::IdempotencyConflict.http_status().as_u16(), 409);
        assert_eq!(AppError::AccountLocked.http_status().as_u16(), 423);
        assert_eq!(AppError::RateLimited.http_status().as_u16(), 429);
        assert_eq!(
            AppError::InvalidTransition("x".into()).http_status().as_u16(),
            409
        );
        assert_eq!(
            AppError::Internal("oops".into()).http_status().as_u16(),
            500
        );
    }

    #[test]
    fn unique_violation_on_asset_label_maps_to_duplicate_error() {
        // Indirect smoke: the From<diesel::result::Error> path is covered by
        // integration tests; here we just verify code() mapping.
        let e = AppError::DuplicateAssetLabel;
        assert_eq!(e.code(), "duplicate_asset_label");
    }

    #[test]
    fn internal_error_is_redacted_for_client() {
        let e = AppError::Internal("secret-database-host=redacted".into());
        let env = e.to_envelope();
        assert_eq!(env["error"].as_str(), Some("internal_error"));
        assert_eq!(env["message"].as_str(), Some(GENERIC_INTERNAL_MESSAGE));
        // The raw detail must never appear in the response envelope.
        let rendered = env.to_string();
        assert!(!rendered.contains("secret-database-host=redacted"));
    }
}

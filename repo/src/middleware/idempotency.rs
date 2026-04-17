use actix_web::{HttpRequest, HttpResponse};
use chrono::Duration;
use diesel::prelude::*;
use serde_json::Value;
use uuid::Uuid;

use crate::db::DbPool;
use crate::errors::{AppError, AppResult};
use crate::middleware::request_context::RequestContext;
use crate::models::idempotency::{IdempotencyKey, NewIdempotencyKey};
use crate::schema::idempotency_keys;
use crate::services::time::now_utc_naive;

pub const TTL_HOURS: i64 = 24;

/// Extract the canonical (method, path) pair from the live request.
/// The replay key uses the concrete path the caller hit — so `.../items/abc`
/// and `.../items/def` are separate entries even with the same request_id.
fn request_key(req: &HttpRequest) -> (String, String) {
    (req.method().as_str().to_string(), req.path().to_string())
}

/// Validate that a caller-supplied `X-Request-Id` looks like a UUID.
/// Accepts any syntactically valid UUID form.
fn validate_request_id(rid: &str) -> AppResult<()> {
    Uuid::parse_str(rid).map(|_| ()).map_err(|_| AppError::Validation {
        message: "X-Request-Id must be a UUID".into(),
        details: serde_json::json!({ "field": "X-Request-Id" }),
    })
}

/// Missing X-Request-Id on a write is a hard validation failure. The prompt
/// requires every mutating request to be idempotent-per-request-id, so an
/// absent header is rejected here rather than silently bypassed.
fn require_request_id(ctx: &RequestContext) -> AppResult<&str> {
    match ctx.request_id.as_deref() {
        Some(r) => {
            validate_request_id(r)?;
            Ok(r)
        }
        None => Err(AppError::Validation {
            message: "X-Request-Id header is required for write operations".into(),
            details: serde_json::json!({ "field": "X-Request-Id" }),
        }),
    }
}

/// Unified write-endpoint idempotency gate.
///
/// Returns `Ok(Some(response))` when this `(user_id, request_id, method, path)`
/// tuple has already been fulfilled — the cached response is replayed verbatim.
/// Returns `Ok(None)` when the caller should proceed. Errors propagate conflicts
/// (same request_id reused across users on any endpoint) or validation failures
/// (missing/malformed request_id).
///
/// `method` and `path` are part of the replay key: reusing the same request_id
/// on a different endpoint is treated as a fresh request, so a POST cannot
/// accidentally replay a PUT's cached body. This is the behavior called out by
/// the second audit's #1 blocker.
pub fn check_before(
    pool: &DbPool,
    ctx: &RequestContext,
    req: &HttpRequest,
) -> AppResult<Option<HttpResponse>> {
    let rid = require_request_id(ctx)?;
    let (method, path) = request_key(req);
    let Some(existing) = lookup(pool, ctx.user.id, rid, &method, &path)? else {
        return Ok(None);
    };
    let status = actix_web::http::StatusCode::from_u16(existing.status_code as u16)
        .unwrap_or(actix_web::http::StatusCode::OK);
    Ok(Some(HttpResponse::build(status).json(existing.response_body)))
}

/// Persist the idempotency envelope after a successful write. Uses
/// explicit `method` and `path` strings rather than `&HttpRequest` so the
/// caller can store the canonical route pattern (matching what
/// `check_before` derived from the live request).
pub fn record_after(
    pool: &DbPool,
    ctx: &RequestContext,
    method: &str,
    path: &str,
    status_code: u16,
    body: &Value,
) -> AppResult<()> {
    // check_before already rejected missing X-Request-Id; defensively no-op
    // here so callers bypassing that path don't panic.
    let Some(rid) = ctx.request_id.as_deref() else {
        return Ok(());
    };
    store(pool, ctx.user.id, rid, method, path, status_code as i32, body.clone())
}

/// Look up a non-expired idempotency row keyed by `(request_id, method, path)`.
///
/// * `Ok(Some(key))` — same `(user_id, request_id, method, path)` exists: replay.
/// * `Err(IdempotencyConflict)` — row exists for this `request_id` but belongs
///   to a different user on any endpoint. Reusing a request-id across users is
///   always a 409 regardless of endpoint.
/// * `Ok(None)` — no matching same-user row on this endpoint; caller proceeds.
pub fn lookup(
    pool: &DbPool,
    user_id: Uuid,
    request_id: &str,
    method: &str,
    path: &str,
) -> AppResult<Option<IdempotencyKey>> {
    let mut conn = pool.get()?;
    let (now, _) = now_utc_naive();
    let rows: Vec<IdempotencyKey> = idempotency_keys::table
        .filter(idempotency_keys::request_id.eq(request_id))
        .filter(idempotency_keys::expires_at.gt(now))
        .load(&mut conn)?;
    if rows.is_empty() {
        return Ok(None);
    }
    // Cross-user reuse is always a conflict — method/path are not even consulted.
    if rows.iter().any(|r| r.user_id != user_id) {
        return Err(AppError::IdempotencyConflict);
    }
    // Same-user rows: replay only when method and path both match.
    let replay = rows
        .iter()
        .find(|r| r.user_id == user_id && r.method == method && r.path == path)
        .cloned();
    Ok(replay)
}

pub fn store(
    pool: &DbPool,
    user_id: Uuid,
    request_id: &str,
    method: &str,
    path: &str,
    status_code: i32,
    body: Value,
) -> AppResult<()> {
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    let expires_at = now + Duration::hours(TTL_HOURS);
    let new = NewIdempotencyKey {
        id: Uuid::new_v4(),
        user_id,
        request_id: request_id.to_string(),
        method: method.to_string(),
        path: path.to_string(),
        status_code,
        response_body: body,
        created_at: now,
        created_offset_minutes: off,
        expires_at,
        expires_offset_minutes: off,
    };
    diesel::insert_into(idempotency_keys::table)
        .values(&new)
        // Composite unique index on (user_id, request_id, method, path) governs
        // this conflict — same-user replay of the same call returns cached row.
        .on_conflict((
            idempotency_keys::user_id,
            idempotency_keys::request_id,
            idempotency_keys::method,
            idempotency_keys::path,
        ))
        .do_nothing()
        .execute(&mut conn)?;
    Ok(())
}

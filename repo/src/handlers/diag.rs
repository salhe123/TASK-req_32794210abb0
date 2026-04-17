//! Diagnostic endpoints — restricted to system admins. These exist so the
//! test harness can verify things that are otherwise only observable through
//! external channels (structured logs, in-memory rate-limit state).
use actix_web::dev::HttpServiceFactory;
use actix_web::{web, HttpMessage, HttpRequest, HttpResponse};
use serde::Deserialize;
use serde_json::json;

use crate::errors::{AppError, AppResult};
use crate::middleware::request_context::RequestContext;
use crate::services::{access_log, rate_limit};

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/__diag")
        .wrap(crate::middleware::auth::Authenticate)
        .route("/access-log", web::get().to(recent))
        .route("/rate-limit/reset", web::post().to(reset_rate_limit))
}

fn require_sysadmin(req: &HttpRequest) -> AppResult<RequestContext> {
    let ext = req.extensions();
    let ctx = ext
        .get::<RequestContext>()
        .cloned()
        .ok_or(AppError::Unauthenticated)?;
    if !ctx.has_permission("system.admin") {
        return Err(AppError::Forbidden);
    }
    Ok(ctx)
}

#[derive(Debug, Deserialize)]
struct RecentQuery {
    limit: Option<usize>,
}

async fn recent(req: HttpRequest, q: web::Query<RecentQuery>) -> AppResult<HttpResponse> {
    let _ = require_sysadmin(&req)?;
    let records = access_log::recent(q.limit.unwrap_or(50).min(200));
    Ok(HttpResponse::Ok().json(json!({ "records": records, "count": records.len() })))
}

#[derive(Debug, Deserialize)]
struct ResetBody {
    key: Option<String>,
}

async fn reset_rate_limit(
    req: HttpRequest,
    body: web::Json<ResetBody>,
) -> AppResult<HttpResponse> {
    let _ = require_sysadmin(&req)?;
    if let Some(k) = &body.key {
        rate_limit::reset(k);
        Ok(HttpResponse::Ok().json(json!({ "status": "reset", "key": k })))
    } else {
        // No key → full reset for tests.
        rate_limit::reset_all();
        Ok(HttpResponse::Ok().json(json!({ "status": "reset_all" })))
    }
}

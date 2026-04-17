use actix_web::dev::HttpServiceFactory;
use actix_web::{web, HttpResponse};
use diesel::prelude::*;
use std::sync::atomic::Ordering;

use crate::db::DbPool;
use crate::metrics::{ERRORS_TOTAL, REQUESTS_TOTAL};
use crate::schema::{outbox_deliveries, sessions};

pub fn scope() -> impl HttpServiceFactory {
    (
        web::resource("/health").route(web::get().to(health)),
        web::resource("/health/ready").route(web::get().to(ready)),
        web::resource("/metrics").route(web::get().to(metrics)),
    )
}

async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({ "status": "ok" }))
}

async fn ready(pool: web::Data<DbPool>) -> HttpResponse {
    // Raw DB errors (with connection URIs, constraint names, etc.) must never
    // be returned in the HTTP body — `/health/ready` is unauthenticated. Log
    // the detail for ops via tracing and return a generic message.
    match pool.get() {
        Ok(mut c) => match diesel::sql_query("SELECT 1").execute(&mut c) {
            Ok(_) => HttpResponse::Ok()
                .json(serde_json::json!({ "status": "ready", "db": "ok" })),
            Err(e) => {
                tracing::error!(error = %e, "readiness probe db query failed");
                HttpResponse::ServiceUnavailable().json(serde_json::json!({
                    "error": "internal_error",
                    "message": crate::errors::GENERIC_INTERNAL_MESSAGE,
                    "details": {}
                }))
            }
        },
        Err(e) => {
            tracing::error!(error = %e, "readiness probe db pool checkout failed");
            HttpResponse::ServiceUnavailable().json(serde_json::json!({
                "error": "internal_error",
                "message": crate::errors::GENERIC_INTERNAL_MESSAGE,
                "details": {}
            }))
        }
    }
}

async fn metrics(pool: web::Data<DbPool>) -> HttpResponse {
    let requests = REQUESTS_TOTAL.load(Ordering::Relaxed);
    let errors = ERRORS_TOTAL.load(Ordering::Relaxed);
    let (outbox_pending, outbox_dead, active_sessions) = match pool.get() {
        Ok(mut c) => {
            let p: i64 = outbox_deliveries::table
                .filter(outbox_deliveries::status.eq("PENDING"))
                .count()
                .get_result(&mut c)
                .unwrap_or(0);
            let d: i64 = outbox_deliveries::table
                .filter(outbox_deliveries::status.eq("DEAD"))
                .count()
                .get_result(&mut c)
                .unwrap_or(0);
            let s: i64 = sessions::table
                .filter(sessions::revoked.eq(false))
                .count()
                .get_result(&mut c)
                .unwrap_or(0);
            (p, d, s)
        }
        Err(_) => (0, 0, 0),
    };
    HttpResponse::Ok().json(serde_json::json!({
        "requestsTotal": requests,
        "errorsTotal": errors,
        "outboxPending": outbox_pending,
        "outboxDead": outbox_dead,
        "activeSessions": active_sessions,
    }))
}

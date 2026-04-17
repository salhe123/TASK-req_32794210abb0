use std::time::Duration;

use diesel::prelude::*;
use tokio::time::sleep;

use crate::db::DbPool;
use crate::schema::{idempotency_keys, sessions};
use crate::services::time::now_utc_naive;

pub fn spawn(pool: DbPool) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = reap_once(&pool) {
                tracing::warn!(error = %e, "reaper pass failed");
            }
            sleep(Duration::from_secs(60)).await;
        }
    });
}

fn reap_once(pool: &DbPool) -> anyhow::Result<()> {
    let mut conn = pool.get()?;
    let (now, _) = now_utc_naive();
    let deleted_keys = diesel::delete(idempotency_keys::table.filter(idempotency_keys::expires_at.lt(now)))
        .execute(&mut conn)?;
    let expired_sessions = diesel::update(
        sessions::table
            .filter(sessions::revoked.eq(false))
            .filter(sessions::expires_at.lt(now)),
    )
    .set(sessions::revoked.eq(true))
    .execute(&mut conn)?;
    if deleted_keys + expired_sessions > 0 {
        tracing::info!(deleted_keys, expired_sessions, "reaper pass");
    }
    Ok(())
}

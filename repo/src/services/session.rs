use base64::Engine;
use chrono::{Duration, NaiveDateTime};
use diesel::prelude::*;
use rand::RngCore;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::db::DbPool;
use crate::errors::{AppError, AppResult};
use crate::models::session::{NewSession, Session};
use crate::schema::sessions;
use crate::services::time::now_utc_naive;

pub const IDLE_TIMEOUT_SECS: i64 = 8 * 60 * 60;

pub fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn mint_token() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

pub fn issue(pool: &DbPool, user_id: Uuid, ttl_secs: i64) -> AppResult<(Session, String)> {
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    let expires_at = now + Duration::seconds(ttl_secs);
    let raw = mint_token();
    let token_hash = hash_token(&raw);
    let new = NewSession {
        id: Uuid::new_v4(),
        user_id,
        token_hash,
        created_at: now,
        created_offset_minutes: off,
        last_activity_at: now,
        expires_at,
        revoked: false,
        last_activity_offset_minutes: off,
        expires_offset_minutes: off,
    };
    let saved: Session = diesel::insert_into(sessions::table)
        .values(&new)
        .get_result(&mut conn)?;
    Ok((saved, raw))
}

pub fn lookup_by_raw(pool: &DbPool, raw: &str) -> AppResult<Session> {
    let mut conn = pool.get()?;
    let h = hash_token(raw);
    let s: Session = sessions::table
        .filter(sessions::token_hash.eq(&h))
        .first(&mut conn)
        .map_err(|e| match e {
            diesel::result::Error::NotFound => AppError::Unauthenticated,
            other => other.into(),
        })?;
    Ok(s)
}

pub fn bump_activity(pool: &DbPool, session_id: Uuid, now: NaiveDateTime) -> AppResult<()> {
    let mut conn = pool.get()?;
    let (_, off) = now_utc_naive();
    diesel::update(sessions::table.filter(sessions::id.eq(session_id)))
        .set((
            sessions::last_activity_at.eq(now),
            sessions::last_activity_offset_minutes.eq(off),
        ))
        .execute(&mut conn)?;
    Ok(())
}

pub fn revoke(pool: &DbPool, session_id: Uuid) -> AppResult<()> {
    let mut conn = pool.get()?;
    diesel::update(sessions::table.filter(sessions::id.eq(session_id)))
        .set(sessions::revoked.eq(true))
        .execute(&mut conn)?;
    Ok(())
}

pub fn is_idle_expired(s: &Session, now: NaiveDateTime) -> bool {
    now - s.last_activity_at > Duration::seconds(IDLE_TIMEOUT_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn idle_expiry() {
        let s = Session {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            token_hash: String::new(),
            created_at: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap(),
            created_offset_minutes: 0,
            last_activity_at: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap(),
            expires_at: NaiveDate::from_ymd_opt(2026, 1, 2).unwrap().and_hms_opt(0, 0, 0).unwrap(),
            revoked: false,
            last_activity_offset_minutes: 0,
            expires_offset_minutes: 0,
        };
        let inside = s.last_activity_at + Duration::seconds(IDLE_TIMEOUT_SECS - 1);
        let past = s.last_activity_at + Duration::seconds(IDLE_TIMEOUT_SECS + 1);
        assert!(!is_idle_expired(&s, inside));
        assert!(is_idle_expired(&s, past));
    }
}

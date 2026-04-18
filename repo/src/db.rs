use std::time::Duration;

use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool};

pub type DbPool = Pool<ConnectionManager<PgConnection>>;

pub fn build_pool(database_url: &str) -> anyhow::Result<DbPool> {
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    // r2d2's default `connection_timeout` is 30 s. When Postgres is down the
    // readiness handler calls `pool.get()` and would block for the full 30 s
    // trying to establish a fresh connection — long enough that the test
    // client's own 30 s HTTP timeout fires first and we never get the 503
    // body. Cap at 2 s so `/api/health/ready` fails fast and returns the
    // proper service-unavailable envelope in the DB-down phase.
    let pool = Pool::builder()
        .max_size(16)
        .connection_timeout(Duration::from_secs(2))
        .build(manager)
        .map_err(|e| anyhow::anyhow!("failed to build db pool: {}", e))?;
    Ok(pool)
}

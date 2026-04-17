use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    pub session_ttl_secs: i64,
    pub kek_path: String,
    pub blob_dir: String,
    pub outbox_export_dir: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: env::var("DATABASE_URL")
                .map_err(|_| anyhow::anyhow!("DATABASE_URL is required"))?,
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
            session_ttl_secs: env::var("SESSION_TTL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(28_800),
            kek_path: env::var("KEK_PATH").unwrap_or_else(|_| "/var/civicops/kek.bin".to_string()),
            blob_dir: env::var("BLOB_DIR").unwrap_or_else(|_| "/var/civicops/blobs".to_string()),
            outbox_export_dir: env::var("OUTBOX_EXPORT_DIR")
                .unwrap_or_else(|_| "/var/civicops/outbox".to_string()),
        })
    }
}

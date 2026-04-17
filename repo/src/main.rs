use actix_web::{web, App, HttpResponse, HttpServer};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use tracing_actix_web::TracingLogger;

mod config;
mod db;
mod errors;
pub mod schema;
pub mod models;
pub mod services;
pub mod middleware;
pub mod handlers;
pub mod metrics;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({ "status": "ok" }))
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .with_current_span(true)
        .with_target(false)
        .init();
}

fn run_migrations(pool: &db::DbPool) -> anyhow::Result<()> {
    let mut conn = pool.get()?;
    conn.run_pending_migrations(MIGRATIONS)
        .map_err(|e| anyhow::anyhow!("migrations failed: {}", e))?;
    Ok(())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();

    let cfg = config::Config::from_env().expect("config");
    services::crypto::init_kek(&cfg.kek_path).expect("init KEK");

    let pool = db::build_pool(&cfg.database_url).expect("db pool");
    run_migrations(&pool).expect("migrations");
    let seed_test = std::env::var("SEED_TEST_FIXTURES").ok().as_deref() == Some("true");
    services::seed::run(
        &pool,
        services::seed::SeedOptions {
            test_fixtures: seed_test,
        },
    )
    .expect("seed");
    services::reaper::spawn(pool.clone());

    let bind = cfg.bind_addr.clone();
    tracing::info!(bind = %bind, "starting civicops");

    HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .wrap(crate::middleware::access_log::AccessLog)
            .wrap(crate::metrics::Metrics::default())
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(cfg.clone()))
            .app_data(web::JsonConfig::default().limit(64 * 1024 * 1024))
            .app_data(web::PayloadConfig::default().limit(64 * 1024 * 1024))
            .route("/health", web::get().to(health))
            .service(handlers::build_api_scope())
    })
    .bind(bind)?
    .run()
    .await
}

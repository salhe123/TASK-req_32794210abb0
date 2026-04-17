use actix_web::{web, Scope};

pub mod auth;
pub mod lost_found;
pub mod assets;
pub mod volunteers;
pub mod packages;
pub mod notifications;
pub mod admin;
pub mod health;
pub mod attachments;
pub mod diag;

pub fn diag_enabled() -> bool {
    std::env::var("CIVICOPS_ENABLE_DIAG").ok().as_deref() == Some("true")
}

pub fn build_api_scope() -> Scope {
    let mut s = web::scope("/api")
        .service(health::scope())
        .service(auth::scope())
        .service(lost_found::scope())
        .service(assets::scope())
        .service(volunteers::scope())
        .service(packages::scope())
        .service(notifications::scope())
        .service(admin::scope());
    // Diagnostic endpoints are opt-in: prod deployments leave them off.
    if diag_enabled() {
        s = s.service(diag::scope());
    }
    s
}

#[cfg(test)]
mod tests {
    use super::diag_enabled;
    use serial_test::serial;

    #[test]
    #[serial]
    fn diag_flag_defaults_off() {
        std::env::remove_var("CIVICOPS_ENABLE_DIAG");
        assert!(!diag_enabled());
    }

    #[test]
    #[serial]
    fn diag_flag_respects_true() {
        std::env::set_var("CIVICOPS_ENABLE_DIAG", "true");
        assert!(diag_enabled());
        std::env::set_var("CIVICOPS_ENABLE_DIAG", "false");
        assert!(!diag_enabled());
        std::env::remove_var("CIVICOPS_ENABLE_DIAG");
    }
}

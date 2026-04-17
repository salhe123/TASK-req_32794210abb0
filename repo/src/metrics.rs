use std::future::{ready, Ready};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::Error;
use futures_util::future::LocalBoxFuture;
use once_cell::sync::Lazy;

pub static REQUESTS_TOTAL: Lazy<AtomicU64> = Lazy::new(|| AtomicU64::new(0));
pub static ERRORS_TOTAL: Lazy<AtomicU64> = Lazy::new(|| AtomicU64::new(0));

#[derive(Default)]
pub struct Metrics;

impl<S, B> Transform<S, ServiceRequest> for Metrics
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = MetricsMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(MetricsMiddleware {
            service: Rc::new(service),
        }))
    }
}

pub struct MetricsMiddleware<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for MetricsMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let srv = self.service.clone();
        Box::pin(async move {
            REQUESTS_TOTAL.fetch_add(1, Ordering::Relaxed);
            let result = srv.call(req).await;
            match &result {
                Ok(res) => {
                    if res.status().is_server_error() || res.status().is_client_error() {
                        ERRORS_TOTAL.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Err(_) => {
                    ERRORS_TOTAL.fetch_add(1, Ordering::Relaxed);
                }
            }
            result
        })
    }
}

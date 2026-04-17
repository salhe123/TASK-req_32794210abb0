use std::future::{ready, Ready};
use std::rc::Rc;
use std::time::Instant;

use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{Error, HttpMessage};
use futures_util::future::LocalBoxFuture;

use crate::middleware::request_context::RequestContext;
use crate::services::access_log;

pub struct AccessLog;

impl<S, B> Transform<S, ServiceRequest> for AccessLog
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = AccessLogMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AccessLogMiddleware {
            service: Rc::new(service),
        }))
    }
}

pub struct AccessLogMiddleware<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for AccessLogMiddleware<S>
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
            let started = Instant::now();
            let request_id = req
                .headers()
                .get("X-Request-Id")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            let method = req.method().as_str().to_string();
            let path = req.path().to_string();

            let facility_id = req
                .query_string()
                .split('&')
                .find_map(|kv| {
                    let mut it = kv.splitn(2, '=');
                    let k = it.next()?;
                    let v = it.next()?;
                    if k == "facilityId" {
                        uuid::Uuid::parse_str(v).ok()
                    } else {
                        None
                    }
                });

            let result = srv.call(req).await;
            let (status, user_id) = match &result {
                Ok(res) => {
                    let user_id = res
                        .request()
                        .extensions()
                        .get::<RequestContext>()
                        .map(|c| c.user.id);
                    (res.status().as_u16(), user_id)
                }
                Err(e) => {
                    let r = e.as_response_error();
                    (r.status_code().as_u16(), None)
                }
            };
            let duration_ms = started.elapsed().as_millis();
            let entry = access_log::build_entry(
                request_id.as_deref(),
                user_id,
                facility_id,
                &method,
                &path,
                status,
                duration_ms,
            );
            tracing::info!(message = "http_request", record = %entry);
            access_log::record(entry);
            result
        })
    }
}

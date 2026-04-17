use std::future::{ready, Ready};
use std::rc::Rc;

use actix_web::body::EitherBody;
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{Error, HttpMessage};
use futures_util::future::LocalBoxFuture;

use crate::errors::AppError;
use crate::middleware::request_context::RequestContext;

pub struct RequirePermissions {
    pub any_of: Vec<&'static str>,
}

impl RequirePermissions {
    pub fn any(perms: &[&'static str]) -> Self {
        Self {
            any_of: perms.to_vec(),
        }
    }
}

impl<S, B> Transform<S, ServiceRequest> for RequirePermissions
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Transform = RequirePermissionsMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RequirePermissionsMiddleware {
            service: Rc::new(service),
            any_of: self.any_of.clone(),
        }))
    }
}

pub struct RequirePermissionsMiddleware<S> {
    service: Rc<S>,
    any_of: Vec<&'static str>,
}

impl<S, B> Service<ServiceRequest> for RequirePermissionsMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let srv = self.service.clone();
        let required = self.any_of.clone();
        Box::pin(async move {
            let ok = {
                let ext = req.extensions();
                let ctx = ext
                    .get::<RequestContext>()
                    .ok_or_else(|| AppError::Unauthenticated)?;
                required.iter().any(|p| ctx.permissions.contains(*p))
            };
            if !ok {
                return Err(AppError::Forbidden.into());
            }
            let res = srv.call(req).await?;
            Ok(res.map_into_left_body())
        })
    }
}

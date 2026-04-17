use std::collections::HashSet;
use std::future::{ready, Ready};
use std::rc::Rc;

use actix_web::body::EitherBody;
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::header;
use actix_web::{Error, HttpMessage};
use diesel::prelude::*;
use futures_util::future::LocalBoxFuture;

use crate::db::DbPool;
use crate::errors::AppError;
use crate::middleware::request_context::RequestContext;
use crate::models::{Role, User};
use crate::schema::{permissions, role_permissions, roles, user_roles, users};
use crate::services::session as session_svc;
use crate::services::time::now_utc_naive;

pub struct Authenticate;

impl<S, B> Transform<S, ServiceRequest> for Authenticate
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Transform = AuthenticateMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AuthenticateMiddleware {
            service: Rc::new(service),
        }))
    }
}

pub struct AuthenticateMiddleware<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for AuthenticateMiddleware<S>
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
        Box::pin(async move {
            let auth_header = req
                .headers()
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            let request_id = req
                .headers()
                .get("X-Request-Id")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            let raw = match auth_header
                .as_deref()
                .and_then(|h| h.strip_prefix("Bearer "))
                .map(|s| s.to_string())
            {
                Some(t) => t,
                None => return Err(AppError::Unauthenticated.into()),
            };

            let pool = req
                .app_data::<actix_web::web::Data<DbPool>>()
                .cloned()
                .ok_or_else(|| AppError::Internal("db pool missing".into()))?;

            let session = session_svc::lookup_by_raw(pool.get_ref(), &raw)?;
            if session.revoked {
                return Err(AppError::SessionExpired.into());
            }
            let (now, _) = now_utc_naive();
            if now > session.expires_at || session_svc::is_idle_expired(&session, now) {
                return Err(AppError::SessionExpired.into());
            }

            let user = load_user(pool.get_ref(), session.user_id)?;
            if !user.is_active {
                return Err(AppError::Forbidden.into());
            }
            let (user_roles, permissions_set) = load_roles_and_permissions(pool.get_ref(), user.id)?;

            session_svc::bump_activity(pool.get_ref(), session.id, now)?;

            let ctx = RequestContext {
                user,
                roles: user_roles,
                permissions: permissions_set,
                session_id: session.id,
                request_id,
            };
            req.extensions_mut().insert(ctx);

            let fut = srv.call(req);
            let res = fut.await?;
            Ok(res.map_into_left_body())
        })
    }
}

fn load_user(pool: &DbPool, user_id: uuid::Uuid) -> Result<User, AppError> {
    let mut conn = pool.get()?;
    let u: User = users::table
        .filter(users::id.eq(user_id))
        .first(&mut conn)
        .map_err(|_| AppError::Unauthenticated)?;
    Ok(u)
}

fn load_roles_and_permissions(
    pool: &DbPool,
    user_id: uuid::Uuid,
) -> Result<(Vec<Role>, HashSet<String>), AppError> {
    let mut conn = pool.get()?;
    let rs: Vec<Role> = roles::table
        .inner_join(user_roles::table.on(user_roles::role_id.eq(roles::id)))
        .filter(user_roles::user_id.eq(user_id))
        .select(roles::all_columns)
        .load(&mut conn)?;
    let role_ids: Vec<uuid::Uuid> = rs.iter().map(|r| r.id).collect();
    let perms: Vec<String> = permissions::table
        .inner_join(role_permissions::table.on(role_permissions::permission_id.eq(permissions::id)))
        .filter(role_permissions::role_id.eq_any(&role_ids))
        .select(permissions::code)
        .load(&mut conn)?;
    Ok((rs, perms.into_iter().collect()))
}

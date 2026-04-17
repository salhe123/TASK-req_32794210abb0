use actix_web::dev::HttpServiceFactory;
use actix_web::{web, HttpMessage, HttpRequest, HttpResponse};
use bigdecimal::BigDecimal;
use diesel::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::str::FromStr;
use uuid::Uuid;

use crate::db::DbPool;
use crate::errors::{AppError, AppResult};
use crate::middleware::idempotency;
use crate::middleware::request_context::RequestContext;
use crate::models::package::{NewPackage, NewPackageVariant, Package, PackageVariant};
use crate::schema::{inventory_items, package_variants, packages, stores, time_slots};
use crate::services::audit as audit_svc;
use crate::services::time::now_utc_naive;

pub const STATUS_DRAFT: &str = "DRAFT";
pub const STATUS_PUBLISHED: &str = "PUBLISHED";
pub const STATUS_UNPUBLISHED: &str = "UNPUBLISHED";

pub const MAX_VARIANTS: i64 = 20;
pub const MAX_INCLUDED_ITEMS: usize = 50;
pub const MAX_INCLUDED_ITEM_NAME: usize = 120;

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/packages")
        .wrap(crate::middleware::auth::Authenticate)
        .service(
            web::resource("")
                .route(web::post().to(create_package))
                .route(web::get().to(list_packages)),
        )
        .service(
            web::resource("/{id}")
                .route(web::get().to(get_package))
                .route(web::put().to(update_package))
                .route(web::delete().to(delete_package)),
        )
        .route("/{id}/publish", web::post().to(publish_package))
        .route("/{id}/unpublish", web::post().to(unpublish_package))
        .service(
            web::resource("/{id}/variants")
                .route(web::get().to(list_variants))
                .route(web::post().to(create_variant)),
        )
        .service(
            web::resource("/{id}/variants/{variantId}")
                .route(web::put().to(update_variant))
                .route(web::delete().to(delete_variant)),
        )
}

fn require_ctx(req: &HttpRequest) -> AppResult<RequestContext> {
    let ext = req.extensions();
    ext.get::<RequestContext>()
        .cloned()
        .ok_or(AppError::Unauthenticated)
}

fn enforce_scope(ctx: &RequestContext, facility_id: Uuid) -> AppResult<()> {
    match ctx.allowed_facilities() {
        None => Ok(()),
        Some(set) if set.contains(&facility_id) => Ok(()),
        _ => Err(AppError::OutOfScope),
    }
}

fn serialize_price(p: &BigDecimal) -> String {
    format!("{:.2}", p.round(2))
}

fn parse_price(s: &str, field: &str) -> AppResult<BigDecimal> {
    let d = BigDecimal::from_str(s).map_err(|_| AppError::Validation {
        message: format!("{} must be a decimal string", field),
        details: json!({ "field": field }),
    })?;
    if d.sign() == bigdecimal::num_bigint::Sign::Minus {
        return Err(AppError::Validation {
            message: format!("{} must be >= 0", field),
            details: json!({ "field": field }),
        });
    }
    Ok(d.round(2))
}

fn serialize_package(p: &Package) -> Value {
    json!({
        "id": p.id,
        "facilityId": p.facility_id,
        "name": p.name,
        "description": p.description,
        "basePrice": serialize_price(&p.base_price),
        "status": p.status,
        "includedItems": p.included_items,
        "createdAt": p.created_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
        "updatedAt": p.updated_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
    })
}

#[derive(Debug, Deserialize, Clone)]
struct IncludedItem {
    name: String,
    #[serde(default = "one")]
    quantity: i32,
}

fn one() -> i32 {
    1
}

fn validate_included_items(items: &[IncludedItem]) -> AppResult<Value> {
    if items.len() > MAX_INCLUDED_ITEMS {
        return Err(AppError::Validation {
            message: format!("at most {} included items per package", MAX_INCLUDED_ITEMS),
            details: json!({ "field": "includedItems", "limit": MAX_INCLUDED_ITEMS }),
        });
    }
    let mut normalized = Vec::with_capacity(items.len());
    for it in items {
        let trimmed = it.name.trim();
        if trimmed.is_empty() {
            return Err(AppError::Validation {
                message: "includedItems[].name required".into(),
                details: json!({ "field": "includedItems.name" }),
            });
        }
        if trimmed.chars().count() > MAX_INCLUDED_ITEM_NAME {
            return Err(AppError::Validation {
                message: format!(
                    "includedItems[].name must be at most {} chars",
                    MAX_INCLUDED_ITEM_NAME
                ),
                details: json!({ "field": "includedItems.name", "limit": MAX_INCLUDED_ITEM_NAME }),
            });
        }
        if it.quantity < 1 {
            return Err(AppError::Validation {
                message: "includedItems[].quantity must be >= 1".into(),
                details: json!({ "field": "includedItems.quantity" }),
            });
        }
        normalized.push(json!({ "name": trimmed, "quantity": it.quantity }));
    }
    Ok(Value::Array(normalized))
}

fn serialize_variant(v: &PackageVariant) -> Value {
    json!({
        "id": v.id,
        "packageId": v.package_id,
        "combinationKey": v.combination_key,
        "price": serialize_price(&v.price),
        "inventoryItemId": v.inventory_item_id,
        "timeSlotId": v.time_slot_id,
        "createdAt": v.created_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
    })
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    #[serde(rename = "facilityId")]
    facility_id: Uuid,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(rename = "basePrice")]
    base_price: String,
    #[serde(default, rename = "includedItems")]
    included_items: Vec<IncludedItem>,
}

async fn create_package(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    body: web::Json<CreateBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["packages.write"]) {
        return Err(AppError::Forbidden);
    }
    enforce_scope(&ctx, body.facility_id)?;
    if body.name.trim().is_empty() {
        return Err(AppError::Validation {
            message: "name required".into(),
            details: json!({ "field": "name" }),
        });
    }
    let base_price = parse_price(&body.base_price, "basePrice")?;
    let included_items = validate_included_items(&body.included_items)?;

    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }

    let mut conn = pool.get()?;
    let fac_ok: Option<Uuid> = stores::table
        .filter(stores::id.eq(body.facility_id))
        .select(stores::id)
        .first(&mut conn)
        .optional()?;
    if fac_ok.is_none() {
        return Err(AppError::Validation {
            message: "facility not found".into(),
            details: json!({ "field": "facilityId" }),
        });
    }
    let (now, off) = now_utc_naive();
    let saved: Package = diesel::insert_into(packages::table)
        .values(NewPackage {
            id: Uuid::new_v4(),
            facility_id: body.facility_id,
            name: body.name.trim().to_string(),
            description: body.description.clone(),
            base_price,
            status: STATUS_DRAFT.to_string(),
            created_at: now,
            created_offset_minutes: off,
            updated_at: now,
            updated_offset_minutes: off,
            included_items,
        })
        .get_result(&mut conn)?;
    audit_svc::write(
        pool.get_ref(),
        audit_svc::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: Some(saved.facility_id),
            entity_type: "package".into(),
            entity_id: saved.id,
            action: "create".into(),
            before_state: None,
            after_state: Some(serialize_package(&saved)),
            request_id: ctx.request_id.clone(),
        },
    )?;
    let body_json = serialize_package(&saved);
    idempotency::record_after(pool.get_ref(), &ctx, "POST", "/api/packages", 201, &body_json)?;
    Ok(HttpResponse::Created().json(body_json))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(rename = "facilityId")]
    facility_id: Option<Uuid>,
    status: Option<String>,
}

async fn list_packages(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    q: web::Query<ListQuery>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["packages.read", "packages.write"]) {
        return Err(AppError::Forbidden);
    }
    let mut conn = pool.get()?;
    let mut query = packages::table.into_boxed();
    if let Some(set) = ctx.allowed_facilities() {
        let ids: Vec<Uuid> = set.into_iter().collect();
        query = query.filter(packages::facility_id.eq_any(ids));
    }
    if let Some(fid) = q.facility_id {
        enforce_scope(&ctx, fid)?;
        query = query.filter(packages::facility_id.eq(fid));
    }
    if let Some(s) = &q.status {
        query = query.filter(packages::status.eq(s));
    }
    let rows: Vec<Package> = query
        .order(packages::created_at.desc())
        .limit(500)
        .load(&mut conn)?;
    let out: Vec<Value> = rows.iter().map(serialize_package).collect();
    Ok(HttpResponse::Ok().json(json!({ "packages": out, "count": out.len() })))
}

async fn load_package(pool: &DbPool, id: Uuid) -> AppResult<Package> {
    let mut conn = pool.get()?;
    packages::table
        .filter(packages::id.eq(id))
        .first(&mut conn)
        .map_err(|e| match e {
            diesel::result::Error::NotFound => AppError::NotFound,
            other => other.into(),
        })
}

async fn get_package(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["packages.read", "packages.write"]) {
        return Err(AppError::Forbidden);
    }
    let p = load_package(pool.get_ref(), path.into_inner()).await?;
    enforce_scope(&ctx, p.facility_id)?;
    let mut conn = pool.get()?;
    let variants: Vec<PackageVariant> = package_variants::table
        .filter(package_variants::package_id.eq(p.id))
        .load(&mut conn)?;
    let mut out = serialize_package(&p);
    if let Value::Object(ref mut m) = out {
        m.insert(
            "variants".into(),
            Value::Array(variants.iter().map(serialize_variant).collect()),
        );
    }
    Ok(HttpResponse::Ok().json(out))
}

#[derive(Debug, Deserialize)]
struct UpdateBody {
    name: Option<String>,
    description: Option<String>,
    #[serde(rename = "basePrice")]
    base_price: Option<String>,
    #[serde(rename = "includedItems")]
    included_items: Option<Vec<IncludedItem>>,
}

async fn update_package(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    body: web::Json<UpdateBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["packages.write"]) {
        return Err(AppError::Forbidden);
    }
    let id = path.into_inner();
    let p = load_package(pool.get_ref(), id).await?;
    enforce_scope(&ctx, p.facility_id)?;
    let new_price = match &body.base_price {
        Some(s) => parse_price(s, "basePrice")?,
        None => p.base_price.clone(),
    };
    let new_included = match &body.included_items {
        Some(items) => validate_included_items(items)?,
        None => p.included_items.clone(),
    };
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();

    let new_name = body.name.clone().unwrap_or_else(|| p.name.clone());
    let new_desc = body
        .description
        .clone()
        .unwrap_or_else(|| p.description.clone());
    let before = serialize_package(&p);
    diesel::update(packages::table.filter(packages::id.eq(id)))
        .set((
            packages::name.eq(&new_name),
            packages::description.eq(&new_desc),
            packages::base_price.eq(&new_price),
            packages::included_items.eq(&new_included),
            packages::updated_at.eq(now),
            packages::updated_offset_minutes.eq(off),
        ))
        .execute(&mut conn)?;
    let updated = load_package(pool.get_ref(), id).await?;
    audit_svc::write(
        pool.get_ref(),
        audit_svc::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: Some(updated.facility_id),
            entity_type: "package".into(),
            entity_id: updated.id,
            action: "update".into(),
            before_state: Some(before),
            after_state: Some(serialize_package(&updated)),
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = serialize_package(&updated);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "PUT",
        &format!("/api/packages/{}", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

async fn delete_package(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["packages.write"]) {
        return Err(AppError::Forbidden);
    }
    let id = path.into_inner();
    let p = load_package(pool.get_ref(), id).await?;
    enforce_scope(&ctx, p.facility_id)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let mut conn = pool.get()?;
    let before = serialize_package(&p);
    diesel::delete(packages::table.filter(packages::id.eq(id))).execute(&mut conn)?;
    audit_svc::write(
        pool.get_ref(),
        audit_svc::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: Some(p.facility_id),
            entity_type: "package".into(),
            entity_id: p.id,
            action: "delete".into(),
            before_state: Some(before),
            after_state: None,
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = json!({ "status": "deleted", "id": id });
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "DELETE",
        &format!("/api/packages/{}", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

async fn publish_package(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["packages.write"]) {
        return Err(AppError::Forbidden);
    }
    let id = path.into_inner();
    let p = load_package(pool.get_ref(), id).await?;
    enforce_scope(&ctx, p.facility_id)?;

    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }

    if p.status != STATUS_DRAFT && p.status != STATUS_UNPUBLISHED {
        return Err(AppError::InvalidTransition(format!(
            "can only publish from DRAFT or UNPUBLISHED, current={}",
            p.status
        )));
    }
    let mut conn = pool.get()?;
    let variants: Vec<PackageVariant> = package_variants::table
        .filter(package_variants::package_id.eq(p.id))
        .load(&mut conn)?;
    for v in &variants {
        if v.price.sign() == bigdecimal::num_bigint::Sign::Minus {
            return Err(AppError::Validation {
                message: "all variant prices must be >= 0".into(),
                details: json!({ "variantId": v.id }),
            });
        }
        if let Some(iid) = v.inventory_item_id {
            let ok: Option<Uuid> = inventory_items::table
                .filter(inventory_items::id.eq(iid))
                .filter(inventory_items::facility_id.eq(p.facility_id))
                .select(inventory_items::id)
                .first(&mut conn)
                .optional()?;
            if ok.is_none() {
                return Err(AppError::Validation {
                    message: "variant references inventory item outside this facility".into(),
                    details: json!({ "variantId": v.id, "inventoryItemId": iid }),
                });
            }
        }
        if let Some(tid) = v.time_slot_id {
            let ok: Option<Uuid> = time_slots::table
                .filter(time_slots::id.eq(tid))
                .filter(time_slots::facility_id.eq(p.facility_id))
                .select(time_slots::id)
                .first(&mut conn)
                .optional()?;
            if ok.is_none() {
                return Err(AppError::Validation {
                    message: "variant references time slot outside this facility".into(),
                    details: json!({ "variantId": v.id, "timeSlotId": tid }),
                });
            }
        }
    }
    let (now, off) = now_utc_naive();
    diesel::update(packages::table.filter(packages::id.eq(id)))
        .set((
            packages::status.eq(STATUS_PUBLISHED),
            packages::updated_at.eq(now),
            packages::updated_offset_minutes.eq(off),
        ))
        .execute(&mut conn)?;
    let updated = load_package(pool.get_ref(), id).await?;
    audit_svc::write(
        pool.get_ref(),
        audit_svc::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: Some(updated.facility_id),
            entity_type: "package".into(),
            entity_id: updated.id,
            action: "publish".into(),
            before_state: Some(serialize_package(&p)),
            after_state: Some(serialize_package(&updated)),
            request_id: ctx.request_id.clone(),
        },
    )?;
    let body_json = serialize_package(&updated);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        &format!("/api/packages/{}/publish", id),
        200,
        &body_json,
    )?;
    Ok(HttpResponse::Ok().json(body_json))
}

async fn unpublish_package(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["packages.write"]) {
        return Err(AppError::Forbidden);
    }
    let id = path.into_inner();
    let p = load_package(pool.get_ref(), id).await?;
    enforce_scope(&ctx, p.facility_id)?;
    if p.status != STATUS_PUBLISHED {
        return Err(AppError::InvalidTransition(format!(
            "can only unpublish from PUBLISHED, current={}",
            p.status
        )));
    }
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    diesel::update(packages::table.filter(packages::id.eq(id)))
        .set((
            packages::status.eq(STATUS_UNPUBLISHED),
            packages::updated_at.eq(now),
            packages::updated_offset_minutes.eq(off),
        ))
        .execute(&mut conn)?;
    let updated = load_package(pool.get_ref(), id).await?;
    let response = serialize_package(&updated);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        &format!("/api/packages/{}/unpublish", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

#[derive(Debug, Deserialize)]
struct CreateVariantBody {
    #[serde(rename = "combinationKey")]
    combination_key: String,
    price: String,
    #[serde(rename = "inventoryItemId")]
    inventory_item_id: Option<Uuid>,
    #[serde(rename = "timeSlotId")]
    time_slot_id: Option<Uuid>,
}

async fn create_variant(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    body: web::Json<CreateVariantBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["packages.write"]) {
        return Err(AppError::Forbidden);
    }
    let pid = path.into_inner();
    let pkg = load_package(pool.get_ref(), pid).await?;
    enforce_scope(&ctx, pkg.facility_id)?;
    if body.combination_key.trim().is_empty() {
        return Err(AppError::Validation {
            message: "combinationKey required".into(),
            details: json!({ "field": "combinationKey" }),
        });
    }
    let price = parse_price(&body.price, "price")?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }

    let mut conn = pool.get()?;
    let count: i64 = package_variants::table
        .filter(package_variants::package_id.eq(pid))
        .count()
        .get_result(&mut conn)?;
    if count + 1 > MAX_VARIANTS {
        return Err(AppError::Validation {
            message: format!("at most {} variants per package", MAX_VARIANTS),
            details: json!({ "limit": MAX_VARIANTS }),
        });
    }

    let dup: Option<Uuid> = package_variants::table
        .filter(package_variants::package_id.eq(pid))
        .filter(package_variants::combination_key.eq(&body.combination_key))
        .select(package_variants::id)
        .first(&mut conn)
        .optional()?;
    if dup.is_some() {
        return Err(AppError::Validation {
            message: "combinationKey already exists for this package".into(),
            details: json!({ "field": "combinationKey" }),
        });
    }

    let (now, off) = now_utc_naive();
    let saved: PackageVariant = diesel::insert_into(package_variants::table)
        .values(NewPackageVariant {
            id: Uuid::new_v4(),
            package_id: pid,
            combination_key: body.combination_key.clone(),
            price,
            inventory_item_id: body.inventory_item_id,
            time_slot_id: body.time_slot_id,
            created_at: now,
            created_offset_minutes: off,
        })
        .get_result(&mut conn)?;
    let response = serialize_variant(&saved);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        &format!("/api/packages/{}/variants", pid),
        201,
        &response,
    )?;
    Ok(HttpResponse::Created().json(response))
}

async fn list_variants(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["packages.read", "packages.write"]) {
        return Err(AppError::Forbidden);
    }
    let pid = path.into_inner();
    let pkg = load_package(pool.get_ref(), pid).await?;
    enforce_scope(&ctx, pkg.facility_id)?;
    let mut conn = pool.get()?;
    let rows: Vec<PackageVariant> = package_variants::table
        .filter(package_variants::package_id.eq(pid))
        .order(package_variants::created_at.asc())
        .load(&mut conn)?;
    let out: Vec<Value> = rows.iter().map(serialize_variant).collect();
    Ok(HttpResponse::Ok().json(json!({ "variants": out, "count": out.len() })))
}

#[derive(Debug, Deserialize)]
struct UpdateVariantBody {
    price: Option<String>,
    #[serde(rename = "inventoryItemId")]
    inventory_item_id: Option<Option<Uuid>>,
    #[serde(rename = "timeSlotId")]
    time_slot_id: Option<Option<Uuid>>,
}

async fn update_variant(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<(Uuid, Uuid)>,
    body: web::Json<UpdateVariantBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["packages.write"]) {
        return Err(AppError::Forbidden);
    }
    let (pid, vid) = path.into_inner();
    let pkg = load_package(pool.get_ref(), pid).await?;
    enforce_scope(&ctx, pkg.facility_id)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let mut conn = pool.get()?;
    let variant: PackageVariant = package_variants::table
        .filter(package_variants::id.eq(vid))
        .filter(package_variants::package_id.eq(pid))
        .first(&mut conn)
        .map_err(|e| match e {
            diesel::result::Error::NotFound => AppError::NotFound,
            other => other.into(),
        })?;

    let new_price = match &body.price {
        Some(s) => parse_price(s, "price")?,
        None => variant.price.clone(),
    };
    let new_inv = body
        .inventory_item_id
        .clone()
        .unwrap_or(variant.inventory_item_id);
    let new_slot = body.time_slot_id.clone().unwrap_or(variant.time_slot_id);

    diesel::update(package_variants::table.filter(package_variants::id.eq(vid)))
        .set((
            package_variants::price.eq(&new_price),
            package_variants::inventory_item_id.eq(new_inv),
            package_variants::time_slot_id.eq(new_slot),
        ))
        .execute(&mut conn)?;
    let updated: PackageVariant = package_variants::table
        .filter(package_variants::id.eq(vid))
        .first(&mut conn)?;
    let response = serialize_variant(&updated);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "PUT",
        &format!("/api/packages/{}/variants/{}", pid, vid),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

async fn delete_variant(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<(Uuid, Uuid)>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["packages.write"]) {
        return Err(AppError::Forbidden);
    }
    let (pid, vid) = path.into_inner();
    let pkg = load_package(pool.get_ref(), pid).await?;
    enforce_scope(&ctx, pkg.facility_id)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let mut conn = pool.get()?;
    let affected = diesel::delete(
        package_variants::table
            .filter(package_variants::id.eq(vid))
            .filter(package_variants::package_id.eq(pid)),
    )
    .execute(&mut conn)?;
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    let response = json!({ "status": "deleted", "id": vid });
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "DELETE",
        &format!("/api/packages/{}/variants/{}", pid, vid),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

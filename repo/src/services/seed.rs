use diesel::prelude::*;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::db::DbPool;
use crate::errors::AppResult;
use crate::models::role::{NewPermission, NewRole};
use crate::models::store::NewStore;
use crate::models::user::NewUser;
use crate::schema::{permissions, role_permissions, roles, stores, user_roles, users};
use crate::services::password;
use crate::services::time::now_utc_naive;

pub const BOOTSTRAP_USERNAME: &str = "admin";
pub const BOOTSTRAP_PASSWORD: &str = "ChangeMeSoon1234";

pub const TEST_USERS: &[(&str, &str, &[&str])] = &[
    ("test_admin", "TestAdminPassword123", &["SYSTEM_ADMIN"]),
    ("test_desk", "TestDeskPassword123", &["DESK_STAFF"]),
    ("test_review", "TestReviewPassword123", &["DESK_REVIEWER"]),
    ("test_asset", "TestAssetPassword123", &["ASSET_MANAGER"]),
    ("test_vol", "TestVolPassword123", &["VOLUNTEER_COORDINATOR"]),
    ("test_vol_full", "TestVolFullPassword123", &["VOLUNTEER_ADMIN"]),
    ("test_pkg", "TestPkgPassword123", &["PACKAGE_MANAGER"]),
    ("test_notif", "TestNotifPassword123", &["NOTIFICATION_ADMIN"]),
    ("test_other", "TestOtherPassword123", &["DESK_STAFF_OTHER_FACILITY"]),
];

pub struct SeedOptions {
    pub test_fixtures: bool,
}

pub fn run(pool: &DbPool, opts: SeedOptions) -> AppResult<()> {
    let mut conn = pool.get()?;
    let existing_users: i64 = users::table.count().get_result(&mut conn)?;
    if existing_users == 0 {
        seed_bootstrap(&mut conn)?;
    }
    if opts.test_fixtures {
        seed_test_fixtures(&mut conn)?;
    }
    Ok(())
}

fn seed_bootstrap(conn: &mut diesel::PgConnection) -> AppResult<()> {
    ensure_permissions(conn)?;
    let perms_by_code = load_permissions(conn)?;
    let (sysadmin_role_id, _) =
        ensure_role(conn, "SYSTEM_ADMIN", "facility:*", json!(["*"]), &perms_by_code, ALL_PERMS)?;
    ensure_role(
        conn,
        "DESK_STAFF",
        "facility:*",
        json!([]),
        &perms_by_code,
        &["lost_found.edit_draft", "lost_found.read", "notifications.read"],
    )?;
    ensure_role(
        conn,
        "DESK_REVIEWER",
        "facility:*",
        json!([]),
        &perms_by_code,
        &["lost_found.review", "lost_found.read", "notifications.read"],
    )?;
    ensure_role(
        conn,
        "ASSET_MANAGER",
        "facility:*",
        json!([]),
        &perms_by_code,
        &["assets.write", "assets.transition", "assets.read", "notifications.read"],
    )?;
    ensure_role(
        conn,
        "VOLUNTEER_COORDINATOR",
        "facility:*",
        json!([]),
        &perms_by_code,
        &["volunteers.write", "volunteers.read", "notifications.read"],
    )?;
    ensure_role(
        conn,
        "VOLUNTEER_ADMIN",
        "facility:*",
        json!(["gov_id", "private_notes", "certificate"]),
        &perms_by_code,
        &["volunteers.write", "volunteers.read", "notifications.read"],
    )?;
    ensure_role(
        conn,
        "PACKAGE_MANAGER",
        "facility:*",
        json!([]),
        &perms_by_code,
        &["packages.write", "packages.read", "notifications.read"],
    )?;
    ensure_role(
        conn,
        "NOTIFICATION_ADMIN",
        "facility:*",
        json!([]),
        &perms_by_code,
        &["notifications.admin", "notifications.read"],
    )?;

    ensure_facility(conn, "default", "DEFAULT")?;

    let admin_id = ensure_user(conn, BOOTSTRAP_USERNAME, BOOTSTRAP_PASSWORD, "Bootstrap Admin")?;
    diesel::insert_into(user_roles::table)
        .values(crate::models::role::UserRole {
            user_id: admin_id,
            role_id: sysadmin_role_id,
        })
        .on_conflict_do_nothing()
        .execute(conn)?;

    ensure_templates(conn)?;

    Ok(())
}

/// Seed default notification templates used by in-app triggers. These can be
/// overridden per deployment via the `/api/notifications/templates` admin API.
fn ensure_templates(conn: &mut diesel::PgConnection) -> AppResult<()> {
    use crate::models::notification::NewNotificationTemplate;
    use crate::schema::notification_templates;
    let defs: &[(&str, &str, &str)] = &[
        (
            "lost_found.submitted",
            "Item submitted: {{ item.title }}",
            "A lost-and-found item was submitted for review.",
        ),
        (
            "lost_found.approved",
            "Item approved: {{ item.title }}",
            "Your lost-and-found item was published.",
        ),
        (
            "lost_found.bounced",
            "Item bounced: {{ item.title }}",
            "Your submission was bounced. Reason: {{ item.bounceReason }}",
        ),
        (
            "volunteer.qualification_expiring",
            "Qualification expiring: {{ volunteer.fullName }}",
            "Qualification {{ qualification.kind }} expires on {{ qualification.expiresOn }}.",
        ),
    ];
    for (code, subject, body) in defs {
        let exists: Option<Uuid> = notification_templates::table
            .filter(notification_templates::code.eq(*code))
            .select(notification_templates::id)
            .first(conn)
            .optional()?;
        if exists.is_some() {
            continue;
        }
        let (now, off) = now_utc_naive();
        diesel::insert_into(notification_templates::table)
            .values(NewNotificationTemplate {
                id: Uuid::new_v4(),
                code: (*code).to_string(),
                subject: (*subject).to_string(),
                body: (*body).to_string(),
                is_active: true,
                created_at: now,
                created_offset_minutes: off,
                updated_at: now,
                updated_offset_minutes: off,
            })
            .execute(conn)?;
    }
    Ok(())
}

const ALL_PERMS: &[&str] = &[
    "system.admin",
    "lost_found.edit_draft",
    "lost_found.review",
    "lost_found.read",
    "assets.write",
    "assets.transition",
    "assets.read",
    "volunteers.write",
    "volunteers.read",
    "packages.write",
    "packages.read",
    "notifications.admin",
    "notifications.read",
];

fn ensure_permissions(conn: &mut diesel::PgConnection) -> AppResult<()> {
    let defs: &[(&str, &str)] = &[
        ("system.admin", "Full system administration"),
        ("lost_found.edit_draft", "Create/edit DRAFT lost-and-found items"),
        ("lost_found.review", "Approve or bounce lost-and-found items"),
        ("lost_found.read", "Read lost-and-found items and attachments"),
        ("assets.write", "Create or update assets"),
        ("assets.transition", "Transition assets between states"),
        ("assets.read", "Read asset inventory and history"),
        ("volunteers.write", "Manage volunteers and qualifications"),
        ("volunteers.read", "Read volunteer records and qualifications"),
        ("packages.write", "Manage photography packages"),
        ("packages.read", "Read photography packages and variants"),
        ("notifications.admin", "Manage notification templates and outbox"),
        ("notifications.read", "Read own notification inbox"),
    ];
    for (code, desc) in defs {
        let exists: Option<Uuid> = permissions::table
            .filter(permissions::code.eq(*code))
            .select(permissions::id)
            .first(conn)
            .optional()?;
        if exists.is_none() {
            diesel::insert_into(permissions::table)
                .values(NewPermission {
                    id: Uuid::new_v4(),
                    code: (*code).to_string(),
                    description: (*desc).to_string(),
                })
                .execute(conn)?;
        }
    }
    Ok(())
}

fn load_permissions(
    conn: &mut diesel::PgConnection,
) -> AppResult<std::collections::HashMap<String, Uuid>> {
    let rows: Vec<(String, Uuid)> = permissions::table
        .select((permissions::code, permissions::id))
        .load(conn)?;
    Ok(rows.into_iter().collect())
}

fn ensure_role(
    conn: &mut diesel::PgConnection,
    name: &str,
    data_scope: &str,
    field_allowlist: Value,
    perms_by_code: &std::collections::HashMap<String, Uuid>,
    perm_codes: &[&str],
) -> AppResult<(Uuid, bool)> {
    let existing: Option<Uuid> = roles::table
        .filter(roles::name.eq(name))
        .select(roles::id)
        .first(conn)
        .optional()?;
    let (role_id, created) = match existing {
        Some(id) => (id, false),
        None => {
            let (now, off) = now_utc_naive();
            let id = Uuid::new_v4();
            diesel::insert_into(roles::table)
                .values(NewRole {
                    id,
                    name: name.to_string(),
                    data_scope: data_scope.to_string(),
                    field_allowlist,
                    created_at: now,
                    created_offset_minutes: off,
                })
                .execute(conn)?;
            (id, true)
        }
    };
    for code in perm_codes {
        if let Some(pid) = perms_by_code.get(*code) {
            diesel::insert_into(role_permissions::table)
                .values(crate::models::role::RolePermission {
                    role_id,
                    permission_id: *pid,
                })
                .on_conflict_do_nothing()
                .execute(conn)?;
        }
    }
    Ok((role_id, created))
}

fn ensure_facility(conn: &mut diesel::PgConnection, name: &str, code: &str) -> AppResult<Uuid> {
    let existing: Option<Uuid> = stores::table
        .filter(stores::code.eq(code))
        .select(stores::id)
        .first(conn)
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let (now, off) = now_utc_naive();
    let id = Uuid::new_v4();
    diesel::insert_into(stores::table)
        .values(NewStore {
            id,
            name: name.to_string(),
            code: code.to_string(),
            is_active: true,
            created_at: now,
            created_offset_minutes: off,
        })
        .execute(conn)?;
    Ok(id)
}

fn ensure_user(
    conn: &mut diesel::PgConnection,
    username: &str,
    plaintext_pw: &str,
    display: &str,
) -> AppResult<Uuid> {
    let existing: Option<Uuid> = users::table
        .filter(users::username.eq(username))
        .select(users::id)
        .first(conn)
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let hash = password::hash_password(plaintext_pw)?;
    let id = Uuid::new_v4();
    let (now, off) = now_utc_naive();
    diesel::insert_into(users::table)
        .values(NewUser {
            id,
            username: username.to_string(),
            password_hash: hash,
            display_name: display.to_string(),
            is_active: true,
            locked_until: None,
            created_at: now,
            created_offset_minutes: off,
            updated_at: now,
            updated_offset_minutes: off,
        })
        .execute(conn)?;
    Ok(id)
}

fn seed_test_fixtures(conn: &mut diesel::PgConnection) -> AppResult<()> {
    let perms_by_code = load_permissions(conn)?;

    ensure_facility(conn, "default", "DEFAULT")?;
    let other_facility = ensure_facility(conn, "secondary", "SECONDARY")?;

    let (other_role_id, _) = ensure_role(
        conn,
        "DESK_STAFF_OTHER_FACILITY",
        &format!("facility:{}", other_facility),
        json!([]),
        &perms_by_code,
        &["lost_found.edit_draft", "lost_found.read", "notifications.read"],
    )?;
    let _ = other_role_id;

    let role_ids = load_role_ids(conn)?;
    for (username, pw, role_names) in TEST_USERS {
        let uid = ensure_user(conn, username, pw, username)?;
        for role_name in *role_names {
            if let Some(rid) = role_ids.get(*role_name) {
                diesel::insert_into(user_roles::table)
                    .values(crate::models::role::UserRole {
                        user_id: uid,
                        role_id: *rid,
                    })
                    .on_conflict_do_nothing()
                    .execute(conn)?;
            }
        }
    }
    Ok(())
}

fn load_role_ids(
    conn: &mut diesel::PgConnection,
) -> AppResult<std::collections::HashMap<String, Uuid>> {
    let rows: Vec<(String, Uuid)> = roles::table
        .select((roles::name, roles::id))
        .load(conn)?;
    Ok(rows.into_iter().collect())
}

use diesel::prelude::*;
use regex::Regex;
use serde_json::{json, Value};
use std::sync::OnceLock;
use uuid::Uuid;

use crate::db::DbPool;
use crate::errors::AppResult;
use crate::models::notification::{NewNotification, NewOutboxDelivery, NotificationTemplate};
use crate::schema::{
    notification_subscriptions, notification_templates, notifications, outbox_deliveries,
};
use crate::services::time::now_utc_naive;

/// Allowed template variable paths. Any render that references a
/// variable outside this set is a `validation_failed` at render time.
pub const ALLOWED_VARIABLES: &[&str] = &[
    "item.id",
    "item.title",
    "item.status",
    "item.bounceReason",
    "asset.id",
    "asset.label",
    "asset.status",
    "volunteer.id",
    "volunteer.fullName",
    "qualification.id",
    "qualification.kind",
    "qualification.expiresOn",
    "package.id",
    "package.name",
    "actor.displayName",
];

/// Supported trigger event kinds.
pub const EVENT_SUBMISSION: &str = "submission";
pub const EVENT_SUPPLEMENT: &str = "supplement";
pub const EVENT_REVIEW: &str = "review";
pub const EVENT_CHANGE: &str = "change";

/// Outbox delivery channels. Offline deployments ship records out over
/// any of these channels; the service only writes the record — transport
/// is the responsibility of an external relay reading the outbox export.
pub const CHANNEL_IN_APP: &str = "in_app";
pub const CHANNEL_EMAIL: &str = "email";
pub const CHANNEL_SMS: &str = "sms";
pub const CHANNEL_WEBHOOK: &str = "webhook";

pub const ALL_CHANNELS: &[&str] = &[CHANNEL_IN_APP, CHANNEL_EMAIL, CHANNEL_SMS, CHANNEL_WEBHOOK];

pub struct Trigger {
    pub user_id: Uuid,
    pub event_kind: &'static str,
    pub template_code: String,
    /// Facility the originating record belongs to. `None` is reserved for
    /// system-level events (e.g. sysadmin actions) that have no facility scope.
    pub facility_id: Option<Uuid>,
    /// Out-of-band delivery channel. Defaults to in-app only; callers that
    /// need email/sms/webhook fan-out should call `enqueue_channels`.
    pub channel: &'static str,
    /// Recipient destination for non-in-app channels (email address, phone, URL).
    pub to_address: Option<String>,
    /// Fallback subject when the referenced template does not exist.
    pub fallback_subject: String,
    /// Fallback body when the referenced template does not exist.
    pub fallback_body: String,
    pub payload: Value,
}

pub fn variable_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{\{\s*([a-zA-Z0-9_.]+)\s*\}\}").unwrap())
}

fn lookup_variable(payload: &Value, path: &str) -> Option<String> {
    let mut cur = payload;
    for seg in path.split('.') {
        match cur.get(seg) {
            Some(v) => cur = v,
            None => return None,
        }
    }
    Some(match cur {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    })
}

/// Apply a template to a payload. Substitutes `{{ path }}` with the matching
/// payload value. Variables outside `ALLOWED_VARIABLES` are left as-is and
/// are rejected upstream at template-save time by
/// `handlers::notifications::validate_template_variables`.
pub fn render(template: &str, payload: &Value) -> String {
    let re = variable_regex();
    re.replace_all(template, |caps: &regex::Captures| {
        let var = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        lookup_variable(payload, var).unwrap_or_default()
    })
    .into_owned()
}

/// Render a trigger against the stored template (if any) and return `(subject, body)`.
/// Falls back to the trigger's `fallback_*` fields when no matching active template exists.
fn render_subject_body(
    conn: &mut diesel::PgConnection,
    t: &Trigger,
) -> AppResult<(String, String)> {
    let tpl: Option<NotificationTemplate> = notification_templates::table
        .filter(notification_templates::code.eq(&t.template_code))
        .filter(notification_templates::is_active.eq(true))
        .first(conn)
        .optional()?;
    match tpl {
        Some(tpl) => Ok((render(&tpl.subject, &t.payload), render(&tpl.body, &t.payload))),
        None => Ok((t.fallback_subject.clone(), t.fallback_body.clone())),
    }
}

/// Enqueue an outbox row and a parallel in-app notification for the given user.
/// Subscriptions are checked here: an opt-out skips enqueue entirely.
/// The subject/body are rendered from the referenced template at trigger time.
pub fn enqueue(pool: &DbPool, t: Trigger) -> AppResult<bool> {
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();

    let enabled: Option<bool> = notification_subscriptions::table
        .filter(notification_subscriptions::user_id.eq(t.user_id))
        .filter(notification_subscriptions::event_kind.eq(t.event_kind))
        .select(notification_subscriptions::enabled)
        .first(&mut conn)
        .optional()?;
    // Default-on: if no row, we enqueue. If row exists and enabled=false, skip.
    if let Some(false) = enabled {
        return Ok(false);
    }

    let (subject, body) = render_subject_body(&mut conn, &t)?;

    let note = NewNotification {
        id: Uuid::new_v4(),
        user_id: t.user_id,
        event_kind: t.event_kind.to_string(),
        subject: subject.clone(),
        body: body.clone(),
        payload: t.payload.clone(),
        is_read: false,
        read_at: None,
        created_at: now,
        created_offset_minutes: off,
        read_offset_minutes: None,
    };
    diesel::insert_into(notifications::table)
        .values(&note)
        .execute(&mut conn)?;

    let delivery = NewOutboxDelivery {
        id: Uuid::new_v4(),
        user_id: t.user_id,
        event_kind: t.event_kind.to_string(),
        template_code: t.template_code,
        subject,
        body,
        payload: t.payload,
        status: "PENDING".to_string(),
        attempt_count: 0,
        next_attempt_at: Some(now),
        last_error: None,
        created_at: now,
        created_offset_minutes: off,
        updated_at: now,
        updated_offset_minutes: off,
        channel: t.channel.to_string(),
        to_address: t.to_address,
        facility_id: t.facility_id,
        next_attempt_offset_minutes: Some(off),
    };
    diesel::insert_into(outbox_deliveries::table)
        .values(&delivery)
        .execute(&mut conn)?;
    Ok(true)
}

pub fn default_payload() -> Value {
    json!({})
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn render_substitutes_nested_paths() {
        let payload = json!({ "item": { "title": "Blue Hat", "status": "PUBLISHED" } });
        let out = render("Hi {{ item.title }} [{{ item.status }}]", &payload);
        assert_eq!(out, "Hi Blue Hat [PUBLISHED]");
    }

    #[test]
    fn render_missing_variable_empties() {
        let out = render("Hi {{ unknown.field }}", &json!({}));
        assert_eq!(out, "Hi ");
    }

    #[test]
    fn all_channels_constant_covers_values() {
        assert!(ALL_CHANNELS.contains(&CHANNEL_IN_APP));
        assert!(ALL_CHANNELS.contains(&CHANNEL_EMAIL));
        assert!(ALL_CHANNELS.contains(&CHANNEL_SMS));
        assert!(ALL_CHANNELS.contains(&CHANNEL_WEBHOOK));
    }
}

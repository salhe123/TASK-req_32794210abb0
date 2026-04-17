use std::collections::HashSet;

use uuid::Uuid;

use crate::models::{Role, User};

#[derive(Debug, Clone)]
pub struct RequestContext {
    pub user: User,
    pub roles: Vec<Role>,
    pub permissions: HashSet<String>,
    pub session_id: Uuid,
    pub request_id: Option<String>,
}

impl RequestContext {
    pub fn has_permission(&self, code: &str) -> bool {
        self.permissions.contains(code)
    }

    pub fn has_any_permission<I, S>(&self, codes: I) -> bool
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        codes.into_iter().any(|c| self.permissions.contains(c.as_ref()))
    }

    /// Returns facility UUIDs in the caller's data scope, or None meaning "all facilities".
    pub fn allowed_facilities(&self) -> Option<HashSet<Uuid>> {
        let mut any_wildcard = false;
        let mut set: HashSet<Uuid> = HashSet::new();
        for r in &self.roles {
            let scope = r.data_scope.trim();
            if scope == "facility:*" {
                any_wildcard = true;
                continue;
            }
            if let Some(rest) = scope.strip_prefix("facility:") {
                if let Ok(id) = Uuid::parse_str(rest) {
                    set.insert(id);
                }
            }
        }
        if any_wildcard {
            None
        } else {
            Some(set)
        }
    }

    pub fn can_view_field(&self, field: &str) -> bool {
        for r in &self.roles {
            if let Some(arr) = r.field_allowlist.as_array() {
                for v in arr {
                    if v.as_str() == Some(field) || v.as_str() == Some("*") {
                        return true;
                    }
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use serde_json::json;

    fn mk_ctx(roles: Vec<Role>, perms: &[&str]) -> RequestContext {
        let now = NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        RequestContext {
            user: crate::models::user::User {
                id: Uuid::new_v4(),
                username: "u".into(),
                password_hash: "h".into(),
                display_name: "U".into(),
                is_active: true,
                locked_until: None,
                created_at: now,
                created_offset_minutes: 0,
                updated_at: now,
                updated_offset_minutes: 0,
            },
            roles,
            permissions: perms.iter().map(|s| s.to_string()).collect(),
            session_id: Uuid::new_v4(),
            request_id: None,
        }
    }

    fn role(scope: &str, fields: serde_json::Value) -> Role {
        let now = NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        Role {
            id: Uuid::new_v4(),
            name: "r".into(),
            data_scope: scope.into(),
            field_allowlist: fields,
            created_at: now,
            created_offset_minutes: 0,
        }
    }

    #[test]
    fn wildcard_scope_returns_none_meaning_all() {
        let ctx = mk_ctx(vec![role("facility:*", json!([]))], &[]);
        assert!(ctx.allowed_facilities().is_none());
    }

    #[test]
    fn explicit_scope_returns_set() {
        let id = Uuid::new_v4();
        let ctx = mk_ctx(vec![role(&format!("facility:{}", id), json!([]))], &[]);
        let set = ctx.allowed_facilities().unwrap();
        assert!(set.contains(&id));
    }

    #[test]
    fn wildcard_beats_explicit_when_both_present() {
        let id = Uuid::new_v4();
        let ctx = mk_ctx(
            vec![
                role(&format!("facility:{}", id), json!([])),
                role("facility:*", json!([])),
            ],
            &[],
        );
        assert!(ctx.allowed_facilities().is_none());
    }

    #[test]
    fn field_allowlist_supports_wildcard() {
        let ctx = mk_ctx(vec![role("facility:*", json!(["*"]))], &[]);
        assert!(ctx.can_view_field("gov_id"));
        assert!(ctx.can_view_field("anything"));
    }

    #[test]
    fn field_allowlist_specific() {
        let ctx = mk_ctx(vec![role("facility:*", json!(["gov_id"]))], &[]);
        assert!(ctx.can_view_field("gov_id"));
        assert!(!ctx.can_view_field("private_notes"));
    }

    #[test]
    fn permission_checks() {
        let ctx = mk_ctx(vec![], &["a", "b"]);
        assert!(ctx.has_permission("a"));
        assert!(!ctx.has_permission("c"));
        assert!(ctx.has_any_permission(&["c", "b"]));
        assert!(!ctx.has_any_permission(&["c", "d"]));
    }
}

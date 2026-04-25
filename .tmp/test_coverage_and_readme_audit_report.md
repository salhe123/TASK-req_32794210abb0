# Test Coverage Audit

## Scope and Method
- Audit mode: static inspection only (no test execution).
- Files inspected: `src/main.rs`, `src/handlers/*.rs`, `tests/*.rs`, `tests/common/mod.rs`, `run_tests.sh`, `README.md`.
- Project type declaration check: README does not explicitly declare a single top-level type keyword (`backend|fullstack|web|android|ios|desktop`) near the top. Inferred type by code/layout: **backend**.

## Backend Endpoint Inventory
Resolved with prefixes from `src/main.rs` (`/health`, `/api`) and `src/handlers/mod.rs` (sub-scopes), including conditional diag scope.

| # | Endpoint (METHOD PATH) |
|---|---|
| 1 | `GET /health` |
| 2 | `GET /api/health` |
| 3 | `GET /api/health/ready` |
| 4 | `GET /api/metrics` |
| 5 | `POST /api/auth/login` |
| 6 | `POST /api/auth/logout` |
| 7 | `POST /api/auth/change-password` |
| 8 | `GET /api/auth/session` |
| 9 | `POST /api/lost-found/items` |
| 10 | `GET /api/lost-found/items` |
| 11 | `GET /api/lost-found/items/{id}` |
| 12 | `PUT /api/lost-found/items/{id}` |
| 13 | `DELETE /api/lost-found/items/{id}` |
| 14 | `POST /api/lost-found/items/{id}/submit` |
| 15 | `POST /api/lost-found/items/{id}/approve` |
| 16 | `POST /api/lost-found/items/{id}/bounce` |
| 17 | `POST /api/lost-found/items/{id}/unpublish` |
| 18 | `POST /api/lost-found/items/{id}/republish` |
| 19 | `GET /api/lost-found/items/{id}/history` |
| 20 | `GET /api/lost-found/items/{id}/attachments` |
| 21 | `POST /api/lost-found/items/{id}/attachments` |
| 22 | `DELETE /api/lost-found/items/{id}/attachments/{attachmentId}` |
| 23 | `POST /api/assets` |
| 24 | `GET /api/assets` |
| 25 | `POST /api/assets/bulk-transition` |
| 26 | `GET /api/assets/{id}` |
| 27 | `GET /api/assets/{id}/history` |
| 28 | `POST /api/assets/{id}/transition` |
| 29 | `GET /api/assets/{id}/maintenance-records` |
| 30 | `POST /api/assets/{id}/maintenance-records` |
| 31 | `POST /api/volunteers` |
| 32 | `GET /api/volunteers` |
| 33 | `GET /api/volunteers/{id}` |
| 34 | `PUT /api/volunteers/{id}` |
| 35 | `DELETE /api/volunteers/{id}` |
| 36 | `GET /api/volunteers/{id}/qualifications` |
| 37 | `POST /api/volunteers/{id}/qualifications` |
| 38 | `DELETE /api/volunteers/{id}/qualifications/{qualificationId}` |
| 39 | `POST /api/packages` |
| 40 | `GET /api/packages` |
| 41 | `GET /api/packages/{id}` |
| 42 | `PUT /api/packages/{id}` |
| 43 | `DELETE /api/packages/{id}` |
| 44 | `POST /api/packages/{id}/publish` |
| 45 | `POST /api/packages/{id}/unpublish` |
| 46 | `GET /api/packages/{id}/variants` |
| 47 | `POST /api/packages/{id}/variants` |
| 48 | `PUT /api/packages/{id}/variants/{variantId}` |
| 49 | `DELETE /api/packages/{id}/variants/{variantId}` |
| 50 | `GET /api/notifications/inbox` |
| 51 | `POST /api/notifications/inbox/{id}/read` |
| 52 | `POST /api/notifications/inbox/mark-all-read` |
| 53 | `GET /api/notifications/templates` |
| 54 | `POST /api/notifications/templates` |
| 55 | `PUT /api/notifications/templates/{id}` |
| 56 | `DELETE /api/notifications/templates/{id}` |
| 57 | `GET /api/notifications/outbox` |
| 58 | `GET /api/notifications/outbox/export` |
| 59 | `POST /api/notifications/outbox/import-results` |
| 60 | `POST /api/notifications/dispatch` |
| 61 | `GET /api/notifications/subscriptions` |
| 62 | `PUT /api/notifications/subscriptions` |
| 63 | `POST /api/admin/users` |
| 64 | `GET /api/admin/users` |
| 65 | `PUT /api/admin/users/{id}` |
| 66 | `PUT /api/admin/users/{id}/unlock` |
| 67 | `POST /api/admin/users/{id}/reset-password` |
| 68 | `POST /api/admin/roles` |
| 69 | `GET /api/admin/roles` |
| 70 | `PUT /api/admin/roles/{id}` |
| 71 | `DELETE /api/admin/roles/{id}` |
| 72 | `GET /api/admin/permissions` |
| 73 | `POST /api/admin/facilities` |
| 74 | `GET /api/admin/facilities` |
| 75 | `PUT /api/admin/facilities/{id}` |
| 76 | `DELETE /api/admin/facilities/{id}` |
| 77 | `GET /api/admin/audit/logs` |
| 78 | `GET /api/admin/idempotency/keys` |
| 79 | `GET /api/__diag/access-log` *(conditional: `CIVICOPS_ENABLE_DIAG=true`)* |
| 80 | `POST /api/__diag/rate-limit/reset` *(conditional: `CIVICOPS_ENABLE_DIAG=true`)* |

## API Test Mapping Table
Legend: `TNM-HTTP` = true no-mock HTTP test, `N/A` = uncovered.

| Endpoint | Covered | Test Type | Test Files | Evidence |
|---|---|---|---|---|
| `GET /health` | yes | TNM-HTTP | `tests/api_smoke.rs`, `tests/api_db_down.rs` | `health_ok`, `liveness_still_200_when_db_down` |
| `GET /api/health` | yes | TNM-HTTP | `tests/api_smoke.rs` | `api_health_scope` |
| `GET /api/health/ready` | yes | TNM-HTTP | `tests/api_smoke.rs`, `tests/api_db_down.rs` | `health_ready_returns_200_when_db_up`, `health_ready_returns_503_when_db_down` |
| `GET /api/metrics` | yes | TNM-HTTP | `tests/api_smoke.rs` | `metrics_payload_shape` |
| `POST /api/auth/login` | yes | TNM-HTTP | `tests/api_auth.rs`, `tests/api_rate_limit.rs` | `login_success_returns_token_and_session_fields`, `login_bucket_returns_429_after_burst` |
| `POST /api/auth/logout` | yes | TNM-HTTP | `tests/api_auth.rs`, `tests/api_unauth.rs` | `logout_revokes_session`, `logout_then_reuse_token_returns_session_expired` |
| `POST /api/auth/change-password` | yes | TNM-HTTP | `tests/api_auth.rs` | `change_password_rejects_short_new_password` |
| `GET /api/auth/session` | yes | TNM-HTTP | `tests/api_auth.rs`, `tests/api_session_ttl.rs` | `session_endpoint_returns_user_identity`, `session_hard_expiry_is_enforced` |
| `POST /api/lost-found/items` | yes | TNM-HTTP | `tests/api_lost_found.rs` | `full_workflow_submit_approve_unpublish_republish_and_audit` |
| `GET /api/lost-found/items` | yes | TNM-HTTP | `tests/api_lost_found_cleanup.rs` | `soft_delete_lost_found_item_hides_from_default_list` |
| `GET /api/lost-found/items/{id}` | yes | TNM-HTTP | `tests/api_lost_found.rs` | `full_workflow_submit_approve_unpublish_republish_and_audit` |
| `PUT /api/lost-found/items/{id}` | yes | TNM-HTTP | `tests/api_lost_found_cleanup.rs` | `update_outside_draft_blocked_with_invalid_transition` |
| `DELETE /api/lost-found/items/{id}` | yes | TNM-HTTP | `tests/api_lost_found_cleanup.rs` | `soft_delete_lost_found_item_hides_from_default_list` |
| `POST /api/lost-found/items/{id}/submit` | yes | TNM-HTTP | `tests/api_lost_found.rs` | `full_workflow_submit_approve_unpublish_republish_and_audit` |
| `POST /api/lost-found/items/{id}/approve` | yes | TNM-HTTP | `tests/api_lost_found.rs`, `tests/api_review_notifications.rs` | `full_workflow_submit_approve_unpublish_republish_and_audit`, `approve_delivers_review_notification_to_submitter` |
| `POST /api/lost-found/items/{id}/bounce` | yes | TNM-HTTP | `tests/api_lost_found.rs`, `tests/api_review_notifications.rs` | `bounce_requires_reason_and_returns_to_draft`, `bounce_delivers_review_notification_with_reason_to_submitter` |
| `POST /api/lost-found/items/{id}/unpublish` | yes | TNM-HTTP | `tests/api_lost_found.rs` | `full_workflow_submit_approve_unpublish_republish_and_audit` |
| `POST /api/lost-found/items/{id}/republish` | yes | TNM-HTTP | `tests/api_lost_found.rs` | `full_workflow_submit_approve_unpublish_republish_and_audit` |
| `GET /api/lost-found/items/{id}/history` | yes | TNM-HTTP | `tests/api_lost_found.rs` | `full_workflow_submit_approve_unpublish_republish_and_audit` |
| `GET /api/lost-found/items/{id}/attachments` | yes | TNM-HTTP | `tests/api_lost_found.rs`, `tests/api_lost_found_cleanup.rs` | `attachment_dedup_per_facility_and_413_on_count`, `delete_attachment_removes_from_listing` |
| `POST /api/lost-found/items/{id}/attachments` | yes | TNM-HTTP | `tests/api_lost_found.rs`, `tests/api_attachment_errors.rs` | `attachment_dedup_per_facility_and_413_on_count`, `unsupported_mime_rejected_with_invalid_attachment` |
| `DELETE /api/lost-found/items/{id}/attachments/{attachmentId}` | yes | TNM-HTTP | `tests/api_lost_found.rs`, `tests/api_lost_found_cleanup.rs` | `cannot_delete_attachment_through_a_different_items_route`, `delete_attachment_removes_from_listing` |
| `POST /api/assets` | yes | TNM-HTTP | `tests/api_assets.rs` | `valid_single_transition_and_history` |
| `GET /api/assets` | yes | TNM-HTTP | `tests/api_access_log.rs`, `tests/api_load.rs` | `access_log_captures_every_required_field`, `concurrent_list_endpoint_throughput` |
| `POST /api/assets/bulk-transition` | yes | TNM-HTTP | `tests/api_assets.rs`, `tests/api_bulk_idempotency.rs` | `bulk_transition_mixes_committed_and_rejected`, `bulk_transition_same_request_id_returns_byte_identical_body` |
| `GET /api/assets/{id}` | **no** | N/A | none | No authenticated request to `/api/assets/{id}` found in `tests/*.rs` |
| `GET /api/assets/{id}/history` | yes | TNM-HTTP | `tests/api_assets.rs` | `valid_single_transition_and_history` |
| `POST /api/assets/{id}/transition` | yes | TNM-HTTP | `tests/api_assets.rs`, `tests/api_asset_transitions.rs` | `valid_single_transition_and_history`, `full_transition_matrix_enforced_at_api` |
| `GET /api/assets/{id}/maintenance-records` | yes | TNM-HTTP | `tests/api_maintenance.rs` | `create_and_list_maintenance_records` |
| `POST /api/assets/{id}/maintenance-records` | yes | TNM-HTTP | `tests/api_maintenance.rs` | `create_and_list_maintenance_records` |
| `POST /api/volunteers` | yes | TNM-HTTP | `tests/api_volunteers.rs`, `tests/api_rbac.rs` | `expiring_within_days_filter_matches_and_triggers_notification`, `volunteer_sensitive_write_rejected_without_allowlist` |
| `GET /api/volunteers` | yes | TNM-HTTP | `tests/api_volunteers.rs` | `expiring_within_days_filter_matches_and_triggers_notification` |
| `GET /api/volunteers/{id}` | yes | TNM-HTTP | `tests/api_volunteers.rs`, `tests/api_rbac.rs` | `volunteer_update_and_soft_delete`, `volunteer_gov_id_masked_by_default_and_full_with_allowlist` |
| `PUT /api/volunteers/{id}` | yes | TNM-HTTP | `tests/api_volunteers.rs`, `tests/api_audit_trail.rs` | `volunteer_update_and_soft_delete`, `volunteer_mutations_produce_audit_rows` |
| `DELETE /api/volunteers/{id}` | yes | TNM-HTTP | `tests/api_volunteers.rs`, `tests/api_audit_trail.rs` | `volunteer_update_and_soft_delete`, `volunteer_mutations_produce_audit_rows` |
| `GET /api/volunteers/{id}/qualifications` | yes | TNM-HTTP | `tests/api_volunteers.rs` | `qualification_delete_removes_row` |
| `POST /api/volunteers/{id}/qualifications` | yes | TNM-HTTP | `tests/api_volunteers.rs`, `tests/api_audit_trail.rs` | `expiring_within_days_filter_matches_and_triggers_notification`, `qualification_create_and_delete_emit_audit_rows` |
| `DELETE /api/volunteers/{id}/qualifications/{qualificationId}` | yes | TNM-HTTP | `tests/api_volunteers.rs`, `tests/api_audit_trail.rs` | `qualification_delete_removes_row`, `qualification_create_and_delete_emit_audit_rows` |
| `POST /api/packages` | yes | TNM-HTTP | `tests/api_packages.rs`, `tests/api_idempotency.rs` | `publish_is_idempotent_with_same_request_id`, `idempotency_key_expires_after_24_hours` |
| `GET /api/packages` | **no** | N/A | none | No authenticated request to `/api/packages` (GET) found in `tests/*.rs` |
| `GET /api/packages/{id}` | yes | TNM-HTTP | `tests/api_packages.rs`, `tests/api_package_extra.rs` | `price_serializes_as_two_decimal_string`, `package_delete_makes_get_return_404` |
| `PUT /api/packages/{id}` | yes | TNM-HTTP | `tests/api_audit_trail.rs` | `package_update_and_delete_emit_audit_rows` |
| `DELETE /api/packages/{id}` | yes | TNM-HTTP | `tests/api_package_extra.rs`, `tests/api_audit_trail.rs` | `package_delete_makes_get_return_404`, `package_update_and_delete_emit_audit_rows` |
| `POST /api/packages/{id}/publish` | yes | TNM-HTTP | `tests/api_packages.rs`, `tests/api_idempotency.rs` | `publish_is_idempotent_with_same_request_id`, `same_user_same_request_id_works_across_different_endpoints` |
| `POST /api/packages/{id}/unpublish` | yes | TNM-HTTP | `tests/api_package_extra.rs` | `unpublish_from_non_published_returns_409` |
| `GET /api/packages/{id}/variants` | yes | TNM-HTTP | `tests/api_package_extra.rs` | `variant_delete_removes_row_and_package_listing_reflects_it` |
| `POST /api/packages/{id}/variants` | yes | TNM-HTTP | `tests/api_packages.rs`, `tests/api_package_extra.rs` | `reject_21st_variant`, `new_variant` |
| `PUT /api/packages/{id}/variants/{variantId}` | yes | TNM-HTTP | `tests/api_package_extra.rs` | `variant_update_changes_price_and_serializes_two_decimal` |
| `DELETE /api/packages/{id}/variants/{variantId}` | yes | TNM-HTTP | `tests/api_package_extra.rs` | `variant_delete_removes_row_and_package_listing_reflects_it` |
| `GET /api/notifications/inbox` | yes | TNM-HTTP | `tests/api_notifications.rs`, `tests/api_review_notifications.rs` | `submit_enqueues_outbox_and_inbox_rows`, `approve_delivers_review_notification_to_submitter` |
| `POST /api/notifications/inbox/{id}/read` | **no** | N/A | none | No request matching `/api/notifications/inbox/{id}/read` found |
| `POST /api/notifications/inbox/mark-all-read` | **no** | N/A | none | No request matching `/api/notifications/inbox/mark-all-read` found |
| `GET /api/notifications/templates` | yes | TNM-HTTP | `tests/api_templates.rs` | `template_create_update_delete_roundtrip` |
| `POST /api/notifications/templates` | yes | TNM-HTTP | `tests/api_templates.rs`, `tests/api_notifications.rs` | `template_create_update_delete_roundtrip`, `template_with_disallowed_variable_rejected` |
| `PUT /api/notifications/templates/{id}` | yes | TNM-HTTP | `tests/api_templates.rs`, `tests/api_audit_trail.rs` | `template_create_update_delete_roundtrip`, `template_crud_emits_audit_rows` |
| `DELETE /api/notifications/templates/{id}` | yes | TNM-HTTP | `tests/api_templates.rs`, `tests/api_audit_trail.rs` | `template_create_update_delete_roundtrip`, `template_crud_emits_audit_rows` |
| `GET /api/notifications/outbox` | yes | TNM-HTTP | `tests/api_notifications.rs`, `tests/api_volunteers.rs` | `submit_enqueues_outbox_and_inbox_rows`, `expiring_within_days_filter_matches_and_triggers_notification` |
| `GET /api/notifications/outbox/export` | yes | TNM-HTTP | `tests/api_notifications.rs` | `export_then_import_ack_moves_to_sent` |
| `POST /api/notifications/outbox/import-results` | yes | TNM-HTTP | `tests/api_notifications.rs` | `outbox_retry_three_failures_then_dead`, `export_then_import_ack_moves_to_sent` |
| `POST /api/notifications/dispatch` | **no** | N/A | none | No request matching `/api/notifications/dispatch` found |
| `GET /api/notifications/subscriptions` | yes | TNM-HTTP | `tests/api_subscriptions.rs` | `subscription_put_then_get_matches_and_persists_across_logins` |
| `PUT /api/notifications/subscriptions` | yes | TNM-HTTP | `tests/api_subscriptions.rs`, `tests/api_notifications.rs` | `subscription_put_then_get_matches_and_persists_across_logins`, `opt_out_skips_enqueue_entirely` |
| `POST /api/admin/users` | yes | TNM-HTTP | `tests/api_admin.rs`, `tests/api_audit_trail.rs` | `admin_user_update_and_reset_password`, `admin_user_and_facility_and_role_mutations_audited` |
| `GET /api/admin/users` | yes | TNM-HTTP | `tests/api_auth.rs`, `tests/api_rbac.rs` | `five_failed_attempts_lock_account_out`, `admin_route_rejects_desk_staff` |
| `PUT /api/admin/users/{id}` | yes | TNM-HTTP | `tests/api_admin.rs`, `tests/api_deactivated_user.rs` | `admin_user_update_and_reset_password`, `deactivated_user_token_rejected_with_forbidden` |
| `PUT /api/admin/users/{id}/unlock` | yes | TNM-HTTP | `tests/api_auth.rs`, `tests/api_audit_trail.rs` | `five_failed_attempts_lock_account_out`, `admin_user_and_facility_and_role_mutations_audited` |
| `POST /api/admin/users/{id}/reset-password` | yes | TNM-HTTP | `tests/api_admin.rs`, `tests/api_audit_trail.rs` | `admin_user_update_and_reset_password`, `admin_user_and_facility_and_role_mutations_audited` |
| `POST /api/admin/roles` | yes | TNM-HTTP | `tests/api_admin.rs`, `tests/api_audit_trail.rs` | `admin_role_crud_roundtrip`, `admin_user_and_facility_and_role_mutations_audited` |
| `GET /api/admin/roles` | yes | TNM-HTTP | `tests/api_admin.rs` | `admin_role_crud_roundtrip` |
| `PUT /api/admin/roles/{id}` | yes | TNM-HTTP | `tests/api_admin.rs` | `admin_role_crud_roundtrip` |
| `DELETE /api/admin/roles/{id}` | yes | TNM-HTTP | `tests/api_admin.rs`, `tests/api_audit_trail.rs` | `admin_role_crud_roundtrip`, `admin_user_and_facility_and_role_mutations_audited` |
| `GET /api/admin/permissions` | yes | TNM-HTTP | `tests/api_admin.rs` | `permissions_catalog_listed` |
| `POST /api/admin/facilities` | yes | TNM-HTTP | `tests/api_admin.rs`, `tests/api_audit_trail.rs` | `admin_facility_crud_roundtrip`, `admin_user_and_facility_and_role_mutations_audited` |
| `GET /api/admin/facilities` | yes | TNM-HTTP | `tests/api_admin.rs` | `admin_facility_crud_roundtrip` |
| `PUT /api/admin/facilities/{id}` | yes | TNM-HTTP | `tests/api_admin.rs`, `tests/api_audit_trail.rs` | `admin_facility_crud_roundtrip`, `admin_user_and_facility_and_role_mutations_audited` |
| `DELETE /api/admin/facilities/{id}` | yes | TNM-HTTP | `tests/api_admin.rs`, `tests/api_audit_trail.rs` | `admin_facility_crud_roundtrip`, `admin_user_and_facility_and_role_mutations_audited` |
| `GET /api/admin/audit/logs` | yes | TNM-HTTP | `tests/api_admin.rs`, `tests/api_rbac.rs`, `tests/api_audit_trail.rs` | `audit_log_list_returns_recent_entries`, `audit_logs_hidden_from_non_admin` |
| `GET /api/admin/idempotency/keys` | yes | TNM-HTTP | `tests/api_idempotency.rs` | `admin_idempotency_keys_list_only_returns_non_expired` |
| `GET /api/__diag/access-log` | yes | TNM-HTTP | `tests/api_access_log.rs` | `access_log_captures_every_required_field` |
| `POST /api/__diag/rate-limit/reset` | yes | TNM-HTTP | `tests/api_auth.rs`, `tests/api_rate_limit.rs` | `reset_rate_limit_bucket`, `login_bucket_returns_429_after_burst` |

## API Test Classification
1. **True No-Mock HTTP**
- All `tests/api_*.rs` that use `tests/common/mod.rs` (`reqwest::blocking::Client` + real network calls to `CIVICOPS_URL`) and `wait_for_service()`.
- Real bootstrapped service path evidence: `tests/common/mod.rs::wait_for_service`, `tests/common/mod.rs::req_json`, `run_tests.sh` (`docker run` app + runner).

2. **HTTP with Mocking**
- **None found** via static scan.

3. **Non-HTTP (unit/integration without HTTP)**
- Inline/unit tests in `src/*` modules, e.g.:
  - `src/services/password.rs` (`policy_*` tests)
  - `src/services/time.rs` (date/time parsing)
  - `src/services/crypto.rs` (encrypt/decrypt)
  - `src/middleware/request_context.rs`
  - `src/errors.rs`
  - `src/handlers/mod.rs`, `src/handlers/assets.rs`, `src/handlers/notifications.rs`

## Mock Detection Results
Static scan for `jest.mock`, `vi.mock`, `sinon.stub`, mock frameworks, and explicit stubbing patterns in `src/` and `tests/` returned no API-path mocks.
- What is mocked: **none detected**.
- Where: N/A.

## Coverage Summary
- Total endpoints: **80**
- Endpoints with HTTP tests hitting real handlers: **75**
- Endpoints with true no-mock API tests: **75**
- HTTP coverage: **93.75%** (`75/80`)
- True API coverage: **93.75%** (`75/80`)

Uncovered endpoints:
1. `GET /api/assets/{id}`
2. `GET /api/packages`
3. `POST /api/notifications/inbox/{id}/read`
4. `POST /api/notifications/inbox/mark-all-read`
5. `POST /api/notifications/dispatch`

## Unit Test Summary
### Backend Unit Tests
- Test files/modules (inline):
  - `src/services/password.rs`
  - `src/services/time.rs`
  - `src/services/crypto.rs`
  - `src/services/session.rs`
  - `src/services/notify.rs`
  - `src/services/rate_limit.rs`
  - `src/middleware/request_context.rs`
  - `src/errors.rs`
  - `src/handlers/mod.rs`
  - `src/handlers/assets.rs`
  - `src/handlers/notifications.rs`
- Modules covered:
  - controllers/handlers: partial (assets transition validator, notifications variable validation, diag flag)
  - services: password/time/crypto/session/notify/rate_limit
  - middleware: request context parsing/permissions
  - repositories: no direct repository-focused unit test layer (logic mostly exercised via API tests)
  - auth/guards/middleware: partially via unit + many API tests
- Important backend modules not directly unit-tested:
  - `src/handlers/lost_found.rs` (unit-level)
  - `src/handlers/volunteers.rs` (unit-level)
  - `src/handlers/packages.rs` (unit-level)
  - `src/handlers/admin.rs` (unit-level)
  - `src/middleware/auth.rs`, `src/middleware/rbac.rs`, `src/middleware/idempotency.rs` (no inline unit block observed)

### Frontend Unit Tests (Strict)
- Frontend test files: **NONE**
- Frameworks/tools detected: **NONE (frontend stack absent)**
- Components/modules covered: N/A
- Important frontend components/modules not tested: N/A (no frontend codebase detected)
- **Frontend unit tests: MISSING**
- CRITICAL GAP rule trigger: **No** (project inferred as backend, not `fullstack`/`web`).

### Cross-Layer Observation
- Backend-only repository; no frontend layer found (`rg` found no frontend source/test artifacts). Balance check across FE/BE is not applicable.

## API Observability Check
- Strong in most API tests: method/path explicit, request payload explicit, and response fields/status asserted.
- Weak areas:
  - `tests/api_load.rs::concurrent_list_endpoint_throughput` and `concurrent_bulk_transition_sweep` emphasize throughput/status over rich response validation.
  - `tests/api_db_contention.rs::unique_label_contention_produces_one_winner` focuses contention outcome counts, limited payload-shape assertions.

## Tests Check
- `run_tests.sh`: Docker-based orchestration (service + DB + runner in containers) — **OK**.
- Local runtime dependency installs required by README for app start/tests: **No** (Docker required, consistent with constraints).

## Test Quality & Sufficiency
- Strengths:
  - Broad API coverage with real HTTP paths and auth/idempotency/security scenarios.
  - Good negative-path coverage (validation failures, RBAC denial, lockout/rate-limit, DB-down readiness).
  - Domain-specific behavior tested (state transitions, attachment limits, outbox lifecycle, audit trails).
- Gaps:
  - Missing handler-level tests for five endpoints listed above.
  - Notification read/dispatch flows are partially unverified end-to-end.

## End-to-End Expectations
- Fullstack FE↔BE E2E requirement: not applicable (backend-only inference).
- Backend E2E/API coverage is strong but not complete due to uncovered endpoints.

## Test Coverage Score (0-100)
**90.8/100**

## Score Rationale
- + Very high endpoint coverage (`93.75%`) with true no-mock HTTP tests.
- + Strong depth across auth, RBAC, idempotency, validation, and failure-path scenarios.
- + Dockerized test harness and deterministic API assertions improve reliability.
- - Five endpoints remain uncovered and should be closed to reach near-complete coverage.
- - A small subset of load/contention tests emphasizes status/throughput over deep payload assertions.

## Key Gaps
1. No test for `GET /api/assets/{id}`.
2. No test for `GET /api/packages`.
3. No test for `POST /api/notifications/inbox/{id}/read`.
4. No test for `POST /api/notifications/inbox/mark-all-read`.
5. No test for `POST /api/notifications/dispatch`.

## Confidence & Assumptions
- Confidence: **High** for endpoint inventory and coverage mapping from static route/test strings.
- Assumptions:
  - Conditional diag routes counted because they are real endpoints in source and enabled in `run_tests.sh` (`CIVICOPS_ENABLE_DIAG=true`).
  - Coverage considered only when an authenticated/valid-path request can reach handler logic (unauth-only probes do not qualify for handler coverage).

---

# README Audit

## README Location Check
- Required file `repo/README.md`: **present**.

## Hard Gate Failures
- None.

## High Priority Issues
- None.

## Medium Priority Issues
1. API documentation section is summary-level only and still does not provide a machine-readable contract (for example OpenAPI).
2. Auth credential section is strong, but role-to-endpoint quick mapping is absent (slows reviewer validation).

## Low Priority Issues
1. README references “API documentation not shipped” without an alternative machine-readable contract (e.g., OpenAPI link), increasing onboarding friction.

## Hard Gate Results (ALL must pass)
- Formatting quality: **PASS**
- Startup instructions (`docker-compose up` for backend/fullstack): **PASS** (`docker-compose up --build -d` present)
- Access method (URL + port): **PASS**
- Verification method: **PASS** (`## Verification (Required)` includes concrete curl steps and expected outcomes)
- Environment rules (no local package/runtime install steps): **PASS**
- Demo credentials (auth present): **PASS** (username/password by role provided)

## README Verdict
**PARTIAL PASS**

## README Confidence
- Confidence: **High** (direct README static review against provided gates).

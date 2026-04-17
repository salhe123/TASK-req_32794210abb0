# Delivery Acceptance and Project Architecture Audit (Static-Only)

## 1. Verdict
- **Overall conclusion: Partial Pass**
- Rationale: The repository is materially aligned with the CivicOps domain and implements substantial backend scope, but has multiple material requirement gaps and security/design defects (including one deployment-model blocker and multiple high-severity issues) that prevent a full pass.

## 2. Scope and Static Verification Boundary
- **What was reviewed**
  - Documentation, config, manifests: `README.md`, `.env.example`, `Dockerfile`, `docker-compose.yml`, `Cargo.toml`, `run_tests.sh`.
  - Entrypoints, route registration, middleware, handlers, services, models, schema, migrations.
  - Test suite structure and static test intent under `tests/`.
- **What was not reviewed/executed**
  - No runtime execution of service, Docker, DB, or tests.
  - No network/external integration checks.
  - No performance benchmarking.
- **Intentionally not executed**
  - Project startup, `docker compose`, `cargo test`, `run_tests.sh`, load tests.
- **Claims requiring manual verification**
  - 50 concurrent users / 200 rps on commodity hardware.
  - Runtime behavior under real production deployment and offline operations.
  - Operational immutability guarantees at DB/infra level beyond application routes.

## 3. Repository / Requirement Mapping Summary
- **Prompt core goal mapped**: Actix-web + Diesel/PostgreSQL backend for offline civic operations covering auth/session, lost-and-found workflows, assets lifecycle, volunteers/qualifications, photography packages, notifications/outbox, and admin RBAC.
- **Main implementation areas mapped**
  - API composition and middleware: `src/main.rs`, `src/handlers/mod.rs`, `src/middleware/*`.
  - Domain handlers: `src/handlers/{auth,lost_found,assets,volunteers,packages,notifications,admin}.rs`.
  - Persistence model: `migrations/*/up.sql`, `src/schema.rs`, `src/models/*`.
  - Security and data handling: `src/services/{password,session,crypto,idempotency,audit,notify}.rs`.
  - Static test evidence: `tests/api_*.rs`, `tests/common/mod.rs`, `run_tests.sh`.

## 4. Section-by-section Review

### 4.1 Hard Gates

#### 4.1.1 Documentation and static verifiability
- **Conclusion: Pass**
- **Rationale:** Startup/config/test instructions exist; entrypoint/routes/docs are statically consistent.
- **Evidence:** `README.md:6`, `README.md:22`, `README.md:37`, `src/main.rs:62`, `src/handlers/mod.rs:18`, `.env.example:1`, `Cargo.toml:41`.

#### 4.1.2 Material deviation from Prompt
- **Conclusion: Fail**
- **Rationale:** Prompt requires a single Docker-deployed service; delivery uses multi-service compose (`app` + `postgres`).
- **Evidence:** `docker-compose.yml:1`, `docker-compose.yml:2`, `docker-compose.yml:17`, `README.md:10`.
- **Manual verification note:** None; this is statically explicit.

### 4.2 Delivery Completeness

#### 4.2.1 Core functional requirements coverage
- **Conclusion: Partial Pass**
- **Rationale:** Most major functional groups are implemented, but there are material compliance gaps (idempotency and security defects; see issues).
- **Evidence:** `src/handlers/auth.rs:21`, `src/handlers/lost_found.rs:66`, `src/handlers/assets.rs:39`, `src/handlers/volunteers.rs:20`, `src/handlers/packages.rs:27`, `src/handlers/notifications.rs:25`, `src/handlers/admin.rs:25`.

#### 4.2.2 End-to-end 0→1 deliverable vs partial/demo
- **Conclusion: Pass**
- **Rationale:** Full project structure, migrations, handlers, services, middleware, and non-trivial integration tests are present.
- **Evidence:** `src/main.rs:39`, `migrations/2026-01-01-000001_identity/up.sql:3`, `migrations/2026-01-01-000007_notifications/up.sql:1`, `tests/api_smoke.rs:5`, `tests/api_lost_found.rs:41`, `tests/api_assets.rs:23`.

### 4.3 Engineering and Architecture Quality

#### 4.3.1 Structure and module decomposition
- **Conclusion: Pass**
- **Rationale:** Domain modules are separated by handler/service/model/middleware/schema; responsibilities are mostly clear.
- **Evidence:** `src/handlers/mod.rs:3`, `src/services/mod.rs:1`, `src/models/mod.rs:1`, `src/middleware/mod.rs:1`.

#### 4.3.2 Maintainability and extensibility
- **Conclusion: Partial Pass**
- **Rationale:** Generally maintainable, but critical contradictions exist in idempotency persistence design and authorization checks.
- **Evidence:** `src/middleware/idempotency.rs:56`, `migrations/2026-01-01-000001_identity/up.sql:76`, `migrations/2026-01-01-000009_audit_round2/up.sql:28`, `src/handlers/lost_found.rs:1054`, `src/services/attachments.rs:179`.

### 4.4 Engineering Details and Professionalism

#### 4.4.1 Error handling, logging, validation, API design
- **Conclusion: Partial Pass**
- **Rationale:** Good structured envelope/logging baseline exists, but high-impact defects remain (missing idempotency on login write path; attachment delete object check gap).
- **Evidence:** `src/errors.rs:91`, `src/errors.rs:139`, `src/main.rs:24`, `src/middleware/access_log.rs:98`, `src/handlers/auth.rs:50`, `src/handlers/lost_found.rs:1054`.

#### 4.4.2 Product-like service vs demo
- **Conclusion: Pass**
- **Rationale:** Delivery resembles a real backend service with migrations, RBAC, persistence, diagnostics, and broad test suite.
- **Evidence:** `src/main.rs:62`, `src/handlers/admin.rs:57`, `src/handlers/health.rs:10`, `tests/api_audit_trail.rs:26`, `tests/api_load.rs:11`.

### 4.5 Prompt Understanding and Requirement Fit

#### 4.5.1 Business goal, scenario, implicit constraints fit
- **Conclusion: Partial Pass**
- **Rationale:** Business domains are implemented with strong alignment, but prompt-critical constraints are violated/at risk (single-service deployment; idempotency contract implementation defects).
- **Evidence:** `src/handlers/lost_found.rs:492`, `src/handlers/assets.rs:433`, `src/handlers/packages.rs:589`, `src/handlers/notifications.rs:449`, `docker-compose.yml:1`, `src/handlers/auth.rs:50`, `migrations/2026-01-01-000001_identity/up.sql:76`.

### 4.6 Aesthetics (frontend-only / full-stack)
- **Conclusion: Not Applicable**
- **Rationale:** Repository is backend-only; no frontend/UI deliverable was found.
- **Evidence:** `Cargo.toml:6`, `src/main.rs:1`, absence of frontend assets/framework manifests.

## 5. Issues / Suggestions (Severity-Rated)

### Blocker

1. **Severity: Blocker**
- **Title:** Deployment model violates single-service prompt constraint
- **Conclusion:** Fail
- **Evidence:** `docker-compose.yml:1`, `docker-compose.yml:2`, `docker-compose.yml:17`, `README.md:10`
- **Impact:** Delivered runtime topology is multi-service (`app` + `postgres`), conflicting with explicit “single Docker-deployed service” acceptance.
- **Minimum actionable fix:** Package PostgreSQL with the service in a single deployment unit as required, or revise acceptance scope explicitly if multi-container on one host is acceptable.

### High

2. **Severity: High**
- **Title:** Idempotency schema contradiction can break per-endpoint request_id behavior
- **Conclusion:** Fail
- **Evidence:** `migrations/2026-01-01-000001_identity/up.sql:76`, `migrations/2026-01-01-000009_audit_round2/up.sql:23`, `migrations/2026-01-01-000009_audit_round2/up.sql:28`, `src/middleware/idempotency.rs:56`, `src/middleware/idempotency.rs:159`
- **Impact:** Legacy unique constraint `UNIQUE(user_id, request_id)` remains while logic expects uniqueness by `(user_id, request_id, method, path)`. Same-user reuse of request_id across endpoints can still violate DB constraint and fail incorrectly.
- **Minimum actionable fix:** Add migration to drop old unique constraint/index on `(user_id, request_id)` and keep only `(user_id, request_id, method, path)` for conflict handling.

3. **Severity: High**
- **Title:** Authentication login write path is not idempotency-gated
- **Conclusion:** Fail
- **Evidence:** `src/handlers/auth.rs:50`, `src/handlers/auth.rs:135`, `src/handlers/auth.rs:147`, `src/middleware/idempotency.rs:32`
- **Impact:** Login creates session and login-attempt records but does not require/request_id idempotency; this violates prompt requirement that all writes be idempotent via client request_id retained 24h/user.
- **Minimum actionable fix:** Apply request-id validation and idempotency replay semantics to login (or formally scope the requirement to exclude auth writes and document it).

4. **Severity: High**
- **Title:** Attachment delete lacks parent-object authorization binding
- **Conclusion:** Fail
- **Evidence:** `src/handlers/lost_found.rs:1054`, `src/handlers/lost_found.rs:1060`, `src/services/attachments.rs:179`, `src/services/attachments.rs:181`
- **Impact:** Delete endpoint takes `{item_id, attachmentId}` but deletion checks only `(attachment_id, facility_id)`, not `parent_id == item_id`. A caller with facility scope can delete other item attachments in same facility by ID.
- **Minimum actionable fix:** Enforce `parent_type` + `parent_id` match during delete (service or handler) and return `404` on mismatch.

### Medium

5. **Severity: Medium**
- **Title:** Package variant linkage integrity is deferred to publish, not enforced at create/update
- **Conclusion:** Partial Pass
- **Evidence:** `src/handlers/packages.rs:589`, `src/handlers/packages.rs:645`, `src/handlers/packages.rs:723`, `src/handlers/packages.rs:472`
- **Impact:** Draft variants can persist cross-facility or invalid logical linkage state until publish-time validation, increasing inconsistent data risk.
- **Minimum actionable fix:** Validate `inventoryItemId`/`timeSlotId` facility ownership in create/update variant paths as well.

6. **Severity: Medium**
- **Title:** Field-level access control appears read-focused; sensitive-field write restrictions are not enforced
- **Conclusion:** Partial Pass
- **Evidence:** `src/middleware/request_context.rs:52`, `src/handlers/volunteers.rs:60`, `src/handlers/volunteers.rs:149`, `src/handlers/volunteers.rs:364`
- **Impact:** Roles without sensitive-field allowlist can still write `govId/privateNotes/certificate` via write permissions, which may conflict with strict field-level control expectations.
- **Minimum actionable fix:** Add explicit field-level write checks for sensitive fields (or document policy that field allowlist controls read-only exposure).

7. **Severity: Medium**
- **Title:** Throughput/concurrency acceptance target is not statically provable
- **Conclusion:** Cannot Confirm Statistically
- **Evidence:** `tests/api_load.rs:9`, `tests/api_load.rs:54`, `src/db.rs:9`
- **Impact:** Delivery includes load tests, but no static proof that required 200 rps / 50 users target is met in required deployment profile.
- **Minimum actionable fix:** Provide benchmark evidence artifacts and profile-specific runbook/results tied to acceptance hardware.

## 6. Security Review Summary

- **Authentication entry points: Partial Pass**
  - Evidence: `src/handlers/auth.rs:23`, `src/services/password.rs:7`, `src/handlers/auth.rs:41`, `src/handlers/auth.rs:96`, `src/services/session.rs:14`.
  - Reasoning: Local username/password, min length, lockout/session checks are implemented; high gap remains on idempotency coverage for login write path.

- **Route-level authorization: Partial Pass**
  - Evidence: `src/handlers/lost_found.rs:68`, `src/handlers/assets.rs:41`, `src/handlers/admin.rs:61`, `src/handlers/notifications.rs:205`.
  - Reasoning: Auth middleware and per-route permission checks are broadly present.

- **Object-level authorization: Fail**
  - Evidence: `src/handlers/lost_found.rs:1054`, `src/services/attachments.rs:179`.
  - Reasoning: Attachment delete does not bind attachment object to requested parent item ID.

- **Function-level authorization: Partial Pass**
  - Evidence: `src/handlers/admin.rs:67`, `src/handlers/notifications.rs:781`, `src/handlers/volunteers.rs:155`.
  - Reasoning: Function-level permission gating is frequent, but sensitive-field write boundaries are not explicit.

- **Tenant/user data isolation: Partial Pass**
  - Evidence: `src/middleware/request_context.rs:30`, `src/handlers/lost_found.rs:104`, `src/handlers/assets.rs:67`, `src/handlers/notifications.rs:423`.
  - Reasoning: Facility scoping is implemented for many resources, but object-level deletion flaw undermines isolation for attachments.

- **Admin/internal/debug protection: Pass**
  - Evidence: `src/handlers/diag.rs:13`, `src/handlers/diag.rs:26`, `src/handlers/mod.rs:14`, `README.md:34`.
  - Reasoning: Diag endpoints are opt-in via env and require `system.admin`.

## 7. Tests and Logging Review

- **Unit tests: Pass (static presence)**
  - Evidence: `src/services/password.rs:39`, `src/services/session.rs:89`, `src/middleware/request_context.rs:66`, `src/services/time.rs:49`, `src/handlers/assets.rs:659`.

- **API/integration tests: Pass (static presence, broad coverage)**
  - Evidence: `tests/api_unauth.rs:7`, `tests/api_rbac.rs:24`, `tests/api_lost_found.rs:41`, `tests/api_assets.rs:23`, `tests/api_notifications.rs:35`, `tests/api_idempotency.rs:7`.

- **Logging categories/observability: Pass**
  - Evidence: `src/main.rs:24`, `src/middleware/access_log.rs:98`, `src/handlers/health.rs:10`, `src/handlers/health.rs:41`, `src/metrics.rs:10`.

- **Sensitive-data leakage risk in logs/responses: Partial Pass**
  - Evidence: `src/errors.rs:120`, `src/errors.rs:141`, `src/services/access_log.rs:35`, `src/handlers/volunteers.rs:72`.
  - Reasoning: Client-facing internal errors are redacted and volunteer sensitive fields are masked by default; still, field-level write controls and object-level delete flaw create downstream sensitive-data governance risk.

## 8. Test Coverage Assessment (Static Audit)

### 8.1 Test Overview
- Unit tests exist in source modules and API/integration tests exist under `tests/api_*.rs`.
- Framework/tooling: Rust `cargo test` with `reqwest` blocking client integration tests.
- Test entry points: `cargo test --all-targets` in `run_tests.sh`.
- Test docs/commands exist.
- Evidence: `Cargo.toml:35`, `tests/common/mod.rs:14`, `run_tests.sh:126`, `README.md:39`.

### 8.2 Coverage Mapping Table

| Requirement / Risk Point | Mapped Test Case(s) | Key Assertion / Fixture / Mock | Coverage Assessment | Gap | Minimum Test Addition |
|---|---|---|---|---|---|
| Auth required (401) | `tests/api_unauth.rs:7`, `tests/api_unauth.rs:42` | `assert_eq!(status, 401)` | sufficient | None | N/A |
| RBAC route denial (403) | `tests/api_rbac.rs:7`, `tests/api_rbac.rs:16` | non-admin denied `/api/admin/*` | sufficient | None | N/A |
| Facility scope isolation | `tests/api_rbac.rs:24` | expects `out_of_scope` for wrong facility | basically covered | No attachment-object mismatch test | Add test deleting attachment with mismatched `{item_id, attachmentId}` expecting 404/403 |
| Lost-found workflow transitions | `tests/api_lost_found.rs:42`, `tests/api_lost_found.rs:128` | submit/approve/bounce/unpublish/republish assertions | sufficient | None | N/A |
| Attachment limits + MIME validation | `tests/api_lost_found.rs:203`, `tests/api_attachment_bytes.rs:34`, `tests/api_attachment_errors.rs:24` | 413 files/bytes, invalid MIME/base64 | sufficient | No parent-binding delete security test | Add mismatch-parent delete negative test |
| Asset state machine + bulk 500 limit | `tests/api_asset_transitions.rs:68`, `tests/api_assets.rs:116` | matrix-based allow/deny + bulk limit | sufficient | None | N/A |
| Package variants <=20, publish checks | `tests/api_packages.rs:55`, `tests/api_packages.rs:77` | 21st rejected; cross-facility link blocked at publish | basically covered | Create/update variant cross-facility validation not covered | Add create/update variant negative tests for foreign facility links |
| Notification outbox retry semantics | `tests/api_notifications.rs:54` | DEAD after repeated failures, attemptCount=4 | basically covered | Prompt interpretation (retry up to 3) ambiguity not explicitly asserted | Add explicit semantic test for retry-count policy docs/behavior |
| Idempotency replay + expiry + cross-user conflict | `tests/api_rbac.rs:91`, `tests/api_bulk_idempotency.rs:22`, `tests/api_idempotency.rs:7` | byte-identical replay, 409 cross-user, 24h expiry | insufficient | No same-user same-request_id across different endpoint test | Add cross-endpoint test to catch legacy unique constraint conflict |
| Sensitive-field masking | `tests/api_rbac.rs:48`, `tests/api_volunteers.rs:123` | masked by default, full with allowlist | basically covered | No tests for sensitive-field write restrictions | Add tests ensuring non-allowlisted users cannot write sensitive fields (if required policy) |
| Logging/access diagnostics | `tests/api_access_log.rs:6` | required access-log keys present | basically covered | No leakage test for sensitive payload in logs | Add negative assertion around sensitive fields not present in access logs |

### 8.3 Security Coverage Audit
- **Authentication:** covered for happy path, invalid credentials, session expiry, revocation (`tests/api_auth.rs:21`, `tests/api_session_ttl.rs:6`, `tests/api_session_idle.rs:6`).
- **Route authorization:** covered for admin-route denial and protected endpoint auth (`tests/api_rbac.rs:7`, `tests/api_unauth.rs:42`).
- **Object-level authorization:** **not meaningfully covered** for attachment parent binding; severe defect could pass current tests.
- **Tenant/data isolation:** partially covered (`tests/api_rbac.rs:24`) but not exhaustive across all object-level operations.
- **Admin/internal protection:** covered for non-admin denial on admin APIs; diag protections are not deeply negative-tested.

### 8.4 Final Coverage Judgment
- **Final Coverage Judgment: Partial Pass**
- Major functional/security flows are broadly tested, but gaps are material: no test for cross-endpoint idempotency constraint contradiction and no test catching attachment parent/object authorization mismatch. Severe defects can remain undetected while current suite passes.

## 9. Final Notes
- Assessment was static-only; no runtime claims are made beyond code/test evidence.
- High-priority remediation should address: deployment-model mismatch, idempotency schema contradiction, login idempotency policy gap, and attachment object-authorization defect.

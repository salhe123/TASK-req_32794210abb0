# Partial Pass Report - CivicOps Static Audit

## Overall Result
- Status: **Partial Pass**
- Audit Type: **Static-only** (no runtime execution, no Docker start, no test execution)

## Why Partial Pass
The backend is broadly complete and aligned with the prompt (auth/session, lost-and-found workflows, assets lifecycle, volunteers/qualifications, packages/variants, notifications/outbox, admin/RBAC), but material defects remain that can cause reliability and security risk.

## Highest-Priority Findings

### 1. High - Lost-and-found category normalization defect can produce 500s
- Evidence: `src/handlers/lost_found.rs:35-43`, `src/handlers/lost_found.rs:255`, `src/handlers/lost_found.rs:443-447`, `migrations/2026-01-01-000008_audit_fixes/up.sql:32-34`, `src/errors.rs:153-161`
- Impact: Values like `"lost "` can pass app validation but fail DB constraint and surface as internal error.
- Minimum fix: Normalize category before persistence and add regression tests for whitespace/format edge cases.

### 2. High - Admin user-role mutations are non-transactional
- Evidence: `src/handlers/admin.rs:120-131`, `src/handlers/admin.rs:195-206`, `migrations/2026-01-01-000001_identity/up.sql:37-40`
- Impact: Partial writes can leave incorrect/empty role assignments under failure conditions.
- Minimum fix: Wrap create/update role assignment logic in a DB transaction and validate role IDs up front.

### 3. Medium - Incomplete unique-violation mapping causes avoidable 500s
- Evidence: `src/errors.rs:153-159`, `migrations/2026-01-01-000001_identity/up.sql:5`, `migrations/2026-01-01-000002_stores_audit/up.sql:4`, `migrations/2026-01-01-000007_notifications/up.sql:3`
- Impact: Duplicate username/role/facility/template operations may return internal errors.
- Minimum fix: Map known DB unique constraints to conflict/validation responses.

### 4. Medium - Timestamp+offset policy not fully uniform
- Evidence: `migrations/2026-01-01-000001_identity/up.sql:9`, `src/schema.rs:51-88`
- Impact: `locked_until` lacks offset companion, reducing timestamp consistency for audit reconstruction.
- Minimum fix: Add `locked_until_offset_minutes` and set it in lock/unlock paths.

### 5. Medium (Suspected Risk) - Dispatch recipient linkage gap
- Evidence: `src/handlers/notifications.rs:695-713`, `src/handlers/notifications.rs:788-796`, `src/services/notify.rs:140-155`
- Impact: Facility-scoped admins can target arbitrary `userId` without explicit user-facility membership validation.
- Minimum fix: Enforce recipient membership/scope validation before enqueue.

## What Is Strong
- Clear module decomposition and API grouping (`src/handlers/mod.rs:18-33`)
- Good auth/session baseline (`src/middleware/auth.rs:69-99`, `src/handlers/auth.rs:106-154`)
- Strong idempotency architecture (`src/middleware/idempotency.rs:60-168`)
- Attachment constraints/dedup/compression implemented (`src/services/attachments.rs:18-21`, `src/services/attachments.rs:82-116`)
- Broad static test suite coverage across major flows (`tests/api_*.rs`)

## Static Boundary / Cannot Confirm
- 50 concurrent users and 200 RPS on commodity hardware cannot be confirmed statically.
- Runtime operational guarantees (long-run behavior, infra controls) require manual verification.

## Recommended Next Acceptance Gate
Treat this delivery as **conditionally acceptable after remediation** of the 2 High findings and at least the 2 core Medium reliability items (unique error mapping and transactional admin mutations), then re-run static review and targeted runtime verification.

# CivicOps Delivery Acceptance Report (Partial Pass)
Date: 2026-04-16

## Overall Verdict
- **Result:** Partial Pass
- **Audit Type:** Static-only (no runtime execution)

## Why Partial Pass
The project is broadly aligned with the required CivicOps backend scope (auth, lost-and-found, assets, volunteers, packages, notifications, admin/RBAC), but has material gaps that prevent full acceptance.

## Severity Summary
- **Blocker:** 1
- **High:** 3
- **Medium:** 3

## Blocker Issue
1. **Deployment model mismatch**
- Prompt requires a single Docker-deployed service.
- Current delivery uses multi-service Compose (`app` + `postgres`).
- Evidence: `docker-compose.yml:1`, `docker-compose.yml:2`, `docker-compose.yml:17`

## High Severity Issues
1. **Idempotency schema contradiction**
- Logic expects `(user_id, request_id, method, path)` semantics.
- Legacy uniqueness on `(user_id, request_id)` still exists and can break cross-endpoint behavior.
- Evidence: `migrations/2026-01-01-000001_identity/up.sql:76`, `migrations/2026-01-01-000009_audit_round2/up.sql:28`, `src/middleware/idempotency.rs:159`

2. **Login write path not idempotency-gated**
- Prompt requires idempotency for all write operations.
- Login creates writes but does not enforce/replay via request_id.
- Evidence: `src/handlers/auth.rs:50`, `src/handlers/auth.rs:135`

3. **Attachment delete object-authorization gap**
- Delete path uses `{item_id, attachmentId}` but service check is attachment+facility only.
- Missing parent-object binding enables same-facility cross-item delete risk.
- Evidence: `src/handlers/lost_found.rs:1054`, `src/services/attachments.rs:179`

## Medium Severity Issues
1. Package variant facility linkage validated only at publish time.
2. Field-level controls appear read-focused; sensitive write control is not explicit.
3. 200 rps / 50 users target cannot be confirmed statically.

## Security Summary
- **Authentication:** Partial Pass
- **Route-level authorization:** Partial Pass
- **Object-level authorization:** Fail
- **Function-level authorization:** Partial Pass
- **Tenant/user isolation:** Partial Pass
- **Admin/internal/debug protection:** Pass

## Test & Coverage Summary (Static)
- Unit and API/integration tests are present and substantial.
- Core auth, RBAC, workflow, attachment limits, transitions, and notification flows are covered.
- Coverage is **Partial Pass** due to critical gaps:
  - no test for cross-endpoint idempotency conflict path,
  - no test for attachment parent/object authorization mismatch.

## File References
- Full detailed audit: `.tmp/delivery_acceptance_architecture_audit.md`
- This summary: `.tmp/partial_pass_report.md`

# CivicOps Operations & Services Management - System Design Document

## 1. Overview

CivicOps is an offline-capable backend service for a local organization that runs a physical lost-and-found desk, manages equipment assets, coordinates volunteers, and sells in-person photography packages across a facility network. The backend is a single Docker-deployed Rust service built on Actix-web (HTTP) and Diesel (PostgreSQL ORM), with no third-party network dependencies. It must sustain ≥50 concurrent users and ≥200 requests/second on commodity hardware.

The service exposes seven resource-based API groups: Authentication & Session, Lost-and-Found, Asset Lifecycle, Volunteer Qualification, Photography Package, Notification Center, and Administration/RBAC.

---

## 2. Architecture

### 2.1 Technology Stack

| Layer | Technology |
|-------|------------|
| Language | Rust |
| HTTP Framework | Actix-web |
| ORM / DB Driver | Diesel |
| Database | PostgreSQL |
| Password Hashing | Argon2id (salted) |
| Field Encryption | AES-256-GCM with local KEK |
| File Storage | Local filesystem (images compressed to 1920px long edge, PDFs as-is) |
| Deployment | Single Docker container, offline, one machine |
| Logging | Structured JSON logs, append-only audit trail |

### 2.2 High-Level Architecture

```text
Clients (operators, reviewers, admins — any HTTP-speaking UI or CLI)
        |
        | REST / JSON over HTTP
        v
Actix-web Router
        |
        v
Middleware: Auth/Session → RBAC/Scope → Idempotency → Rate-limit → Audit
        |
        v
Handler Layer (per resource group)
        |
        v
Service Layer (state machines, validation, template rendering, crypto)
        |
        v
Data Access Layer (Diesel)
        |
        v
PostgreSQL  +  Local File Storage  +  Outbox Export Queue
```

### 2.3 Module Structure

- `auth` — local username/password login, session issuance, lockout, idle expiry
- `lost_found` — item CRUD, draft → in_review → published workflow, attachments with SHA-256 dedup
- `assets` — state machine, label uniqueness, bulk actions
- `volunteers` — volunteer records, qualifications, sensitive-field encryption & masking
- `packages` — photography package CRUD, variants, publish/unpublish, inventory/slot linkage
- `notifications` — in-app messages, outbox deliveries, template rendering, retry, export/import
- `admin` — users, roles, permissions, facilities, audit log queries, idempotency inspection
- `common` — request_id idempotency, pagination, validators, error envelope, time/timezone helpers

---

## 3. Security Model

### 3.1 Authentication

- Local username + password only (no OAuth, no external IdP)
- Password minimum 12 characters, salted hash (Argon2id)
- Lockout after 5 failed attempts within a rolling 15-minute window
- Session expires after 8 hours of idle time
- Session tokens are opaque, server-side stored, bound to user + issue-time

### 3.2 Roles & Permissions (RBAC)

Roles are composed of (a) resource-action permissions, (b) data scope, and (c) a field allowlist.

| Role | Description |
|------|-------------|
| DESK_STAFF | Create/edit lost-and-found items in draft; read within facility |
| DESK_REVIEWER | Approve/reject lost-and-found items to PUBLISHED |
| ASSET_CUSTODIAN | Operate asset lifecycle transitions within facility |
| VOLUNTEER_COORDINATOR | Manage volunteers and qualifications; access masked sensitive fields |
| PACKAGE_MANAGER | CRUD photography packages and variants, publish/unpublish |
| NOTIFICATION_ADMIN | Manage templates, subscriptions, outbox export/import |
| SYSTEM_ADMIN | All resources, all facilities, full field access, idempotency & audit inspection |

Data scope values:
- `facility:<id>` — single-facility scope; queries are filtered by `facility_id`
- `facility:*` — all facilities

Field allowlist controls which sensitive fields are returned un-masked (see §3.3).

### 3.3 Data Protection

- Sensitive fields — government ID numbers, certificates, private volunteer notes — are encrypted at rest with AES-256-GCM using a local key-encryption-key (KEK) loaded at startup.
- By default, sensitive fields are returned masked (e.g., last 4 characters only); full value is only returned when the caller's role grants the field in its allowlist.
- Audit logs are append-only; no UPDATE/DELETE paths are exposed.
- File uploads are scanned for MIME/type, size-capped (10 files / 25 MB per parent record), and images are re-encoded locally before hashing.
- Idempotency keys may cache responses that contain masked values — replayed keys return the originally-authorized response verbatim.

---

## 4. Core Modules

### 4.1 Lost-and-Found Module (addresses Q1, Q2)

**Workflow states:** `DRAFT → IN_REVIEW → PUBLISHED`, plus `UNPUBLISHED` (reversible) and soft-deleted `DELETED`.

- DESK_STAFF creates/edits items in DRAFT, then submits to IN_REVIEW. Submission validates all required fields.
- DESK_REVIEWER approves IN_REVIEW → PUBLISHED, or bounces back to DRAFT with a required reason.
- UNPUBLISHED is reachable from PUBLISHED and can return to PUBLISHED without re-review.
- DELETE is a soft delete; rows remain for audit.
- Every transition writes an audit_log row (actor, timestamp, reason).

**Required occurrence fields:**
- `event_date` formatted MM/DD/YYYY
- `event_time` 12-hour with AM/PM (e.g., `02:30 PM`)
- `location_text` free text, max 200 characters, no map lookup

**Classification:**
- `category` — single enum value (required)
- `tags` — 0 to 10 strings, each 2–24 characters

**Attachments (Q2):**
- Up to 10 files per item, 25 MB aggregate cap
- Accepted types: images (JPEG/PNG/WebP) and PDF
- Images are compressed locally to max 1920 px on the long edge before hashing
- SHA-256 fingerprint computed on the final stored bytes
- Within a facility, duplicate fingerprints reuse the existing blob (reference-counted); a new `attachments` row is created pointing to the same storage path
- Exceeding either the 10-file or 25 MB cap returns 413 with the specific limit hit

### 4.2 Asset Lifecycle Module (addresses Q3)

**States:** `INTAKE`, `ASSIGNMENT`, `LOAN`, `TRANSFER`, `MAINTENANCE`, `REPAIR`, `INVENTORY_COUNT`, `DISPOSAL` (terminal).

**Transition table:**

| From | Allowed To |
|------|-----------|
| INTAKE | ASSIGNMENT, INVENTORY_COUNT |
| ASSIGNMENT | LOAN, TRANSFER, MAINTENANCE, INVENTORY_COUNT |
| LOAN | ASSIGNMENT, MAINTENANCE |
| TRANSFER | ASSIGNMENT (at destination facility) |
| MAINTENANCE | REPAIR, ASSIGNMENT |
| REPAIR | ASSIGNMENT, DISPOSAL |
| INVENTORY_COUNT | prior state |
| DISPOSAL | (terminal) |

**Bulk transitions:**
- Up to 500 asset IDs per request
- Exactly one target state per request
- Each row validated against the table; invalid rows reported per-ID; valid rows commit in a single Diesel transaction

**Label uniqueness:** barcode/QR `asset_label` is unique per facility, enforced by a composite unique index `(facility_id, asset_label)`.

Every transition writes an `asset_events` row (immutable history).

### 4.3 Volunteer Qualification Module

- CRUD volunteers and qualification records (certificate name, issuer, expiry date)
- Private volunteer notes, government ID, and certificate numbers are encrypted at rest and masked by default
- Expiring qualifications (within 30 days) surface in a filter and trigger a notification event

### 4.4 Photography Package Module (addresses Q4)

- CRUD packages with `base_price` (USD, NUMERIC(10,2), ≥0), `is_published` flag, and list of `included_items`
- `package_variants` — up to 20 combinations per package, each with a unique combination key, own `price`, optional `inventory_item_id`, optional `time_slot_id` (all within the same facility)
- Publish validates: every variant has a price; any linked inventory item and time slot exist and belong to the same facility
- Unpublish is reversible; variants are not deleted
- No external calendar — time slots are internal rows only

### 4.5 Notification & Messaging Center (addresses Q5)

- Two delivery paths: **in-app messages** (stored, read-state tracked) and **outbox deliveries** for email/SMS/webhook (offline export)
- Triggers: `submission`, `supplement`, `review`, `change` events fire template rendering
- Templates render variables only from an approved allowlist of fields (no arbitrary field access)
- `outbox_deliveries` lifecycle: `PENDING → SENT` (on successful export/ack) or `PENDING → FAILED → PENDING (retry)` up to 3 retries with exponential backoff (1 min, 5 min, 30 min); after 3 failures: `DEAD`
- Export endpoint returns PENDING rows as JSON Lines for an offline mailer; on re-import, matching rows are marked SENT and a send-log entry is appended
- Per-user subscription preferences are evaluated at enqueue time; opt-outs skip enqueue entirely

### 4.6 Administration & RBAC Module (addresses Q6)

- Manage users, roles, permissions, facilities
- Role composition: resource-action permissions + data scope (`facility:<id>` | `facility:*`) + field allowlist
- Audit log read API (filter by user, resource, date range); no mutation endpoints
- Idempotency inspection endpoint for debugging replayed keys

---

## 5. Data Model

All primary keys are UUID v4. Timestamps are stored as local time with the originating UTC offset (`TIMESTAMP` + `offset_minutes SMALLINT`).

**Core tables:**
- `users` — id, username, password_hash (Argon2id), failed_attempts, locked_until, last_activity_at
- `roles` — id, name, data_scope, field_allowlist (JSONB)
- `permissions` — id, resource, action
- `role_permissions` — role_id, permission_id
- `user_roles` — user_id, role_id
- `stores` — id, name, timezone_offset_minutes (facilities)
- `audit_logs` — id, actor_user_id, action, resource_type, resource_id, facility_id, before (JSONB), after (JSONB), created_at

**Domain tables:**
- `lost_found_items` — id, facility_id, status, category, tags (TEXT[]), event_date, event_time, location_text, submitted_by, reviewed_by, created_at, updated_at
- `attachments` — id, parent_type, parent_id, facility_id, storage_path, mime_type, size_bytes, sha256 (BYTEA), ref_count
- `assets` — id, facility_id, asset_label, state, created_at, updated_at
- `asset_events` — id, asset_id, from_state, to_state, actor_user_id, reason, created_at (immutable)
- `maintenance_records` — id, asset_id, started_at, completed_at, technician_user_id, notes
- `volunteers` — id, facility_id, display_name, gov_id_encrypted, private_notes_encrypted, created_at
- `qualifications` — id, volunteer_id, name, issuer, certificate_encrypted, issued_on, expires_on
- `packages` — id, facility_id, name, base_price NUMERIC(10,2), is_published, included_items (JSONB)
- `package_variants` — id, package_id, combination_key, price NUMERIC(10,2), inventory_item_id, time_slot_id
- `notifications` — id, recipient_user_id, template_id, payload (JSONB), read_at, created_at
- `outbox_deliveries` — id, channel, recipient, template_id, rendered_body, variables (JSONB), status, attempt_count, next_attempt_at, last_error
- `idempotency_keys` — (user_id, request_id) PK, method, path, response_status, response_body, created_at, expires_at (24h)

**High-selectivity indexes:**
- `(facility_id, status)` on `lost_found_items` and `assets`
- `(facility_id, asset_label)` UNIQUE on `assets`
- `created_at` on `audit_logs`, `outbox_deliveries`, `asset_events`
- `(parent_type, parent_id)` on `attachments`
- `(facility_id, sha256)` on `attachments` (dedup lookup)

---

## 6. Cross-Cutting Concerns

### 6.1 Idempotency (addresses Q6)

- Every write endpoint requires header `X-Request-Id: <UUID>`
- On first receipt, the handler runs, then the response (status + body) is stored under `(user_id, request_id)` with 24-hour TTL
- Replays with the same (user_id, request_id) return the stored response verbatim — preserving original masking
- A different user replaying another user's request_id is rejected with 409
- Expired keys are reaped by a background task

### 6.2 State Machines

- Lost-and-found and Asset lifecycle transitions are validated by a pure function before any write
- Invalid transitions return 409 with `{ "error": "invalid_transition", "from": "...", "to": "..." }`

### 6.3 Audit Trail

- All mutations emit an `audit_logs` row with before/after JSONB snapshots
- The audit_logs table has no UPDATE/DELETE endpoints; rows are append-only

### 6.4 Concurrency & Performance

- Actix-web async handlers; PostgreSQL connection pool (deadpool/diesel-async or r2d2)
- Target: ≥50 concurrent users, ≥200 RPS on commodity hardware
- Bulk asset transitions wrapped in a single DB transaction

### 6.5 Observability

- Structured JSON logs: request_id, user_id, facility_id, method, path, status, duration_ms
- `/health` — process liveness
- `/health/ready` — DB connectivity + migrations applied
- `/metrics` — basic counters (requests, errors, outbox queue depth) for offline review

---

## 7. Error Envelope

All error responses share a common JSON shape:
```json
{ "error": "<code>", "message": "<human>", "details": { ... } }
```

| HTTP | Code examples |
|------|---------------|
| 400 | `validation_failed`, `invalid_attachment` |
| 401 | `unauthenticated`, `session_expired` |
| 403 | `forbidden`, `out_of_scope` |
| 404 | `not_found` |
| 409 | `invalid_transition`, `duplicate_asset_label`, `idempotency_conflict` |
| 413 | `attachment_limit_exceeded` (includes which limit) |
| 423 | `account_locked` |
| 429 | `rate_limited` |
| 500 | `internal_error` |

---

## 8. Deployment

- Single Docker image, runs on one machine
- PostgreSQL runs alongside (either a second container via compose or external local DB)
- No outbound network calls
- Config via environment variables: `DATABASE_URL`, `KEK_PATH`, `BIND_ADDR`, `SESSION_TTL_SECS`
- Migrations shipped as Diesel migration files, applied at startup
- Outbox exports written to a mounted volume for offline delivery

---

## 9. Testing Strategy

All tests run inside Docker via `run_tests.sh` — no reliance on host-installed Rust or PostgreSQL. The script builds the service image, boots a disposable PostgreSQL container, applies migrations, runs the suites, and tears the stack down. Tests are written in Rust (project's native language); no Bash test suites.

Because this is a backend-only (server) project, the suite has two tiers — unit and API — with no frontend-component or SPA tier. The `API_tests` folder is kept; the frontend-only exception does not apply.

**Unit tests (Rust `#[cfg(test)]`)** — target ≥90% line coverage, measured with `cargo tarpaulin` inside the test container.
- State-machine transition tables for lost-and-found (`DRAFT → IN_REVIEW → PUBLISHED → UNPUBLISHED`) and assets (8-state table from §4.2)
- Template rendering against the approved variable allowlist (rejects unapproved field access)
- Attachment pipeline: image re-encode to 1920 px long edge, SHA-256 computation, aggregate 25 MB / 10-file caps
- Field masking (last-4) and AES-256-GCM encrypt/decrypt round-trips
- Password policy (≥12 chars, Argon2id verify), lockout counter math (5 in 15 min), session idle expiry (8 h)

**API tests (Rust, `reqwest` against the running Actix-web container)** — must call real HTTP endpoints and assert on full JSON payload shape, not status codes or regex alone.
- Auth: login → session → logout; lockout after 5 failed attempts; idle-expiry rejection
- Lost-and-found: full workflow (create → submit → approve → unpublish → republish → delete) with payload assertions on every transition and on `/history`
- Attachments: multipart upload, 413 on size/count breach with `details.limit` field, dedup path returns `deduplicated: true` with matching `sha256`
- Assets: per-asset transitions hitting each row of the transition table; bulk-transition with 500 IDs mixing valid and invalid, asserting both `committed` count and per-ID `rejected[]` entries
- Packages: publish validation (variant price, cross-facility inventory/slot rejection); 21st variant returns 400
- Notifications: outbox export returns JSON Lines; retry increments `attempt_count` with correct `next_attempt_at`; 4th failure lands in `DEAD`; subscription opt-out skips enqueue
- RBAC: single-facility scope filters list endpoints; sensitive fields return masked unless allowlisted; cross-user `X-Request-Id` replay → 409 `idempotency_conflict`; same-user replay returns byte-identical cached response
- Health: `/health`, `/health/ready` (DB down scenario), `/metrics` payload shape

**Load check** — a `k6` or Rust-native driver run inside the Docker network verifies ≥200 RPS and ≥50 concurrent sessions on the bulk-transition and list endpoints.

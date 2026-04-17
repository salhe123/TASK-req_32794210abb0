## Business Logic Questions Log

### 1. How does the lost-and-found publication workflow work?
- **Problem:** The prompt specifies a draft → in_review → published flow plus update/unpublish/delete, but does not define who can transition each state or what validation gates each step.
- **My Understanding:** Items move through a review gate before becoming visible, with role-based transition rights and immutable history.
- **Solution:** States: DRAFT → IN_REVIEW → PUBLISHED, with UNPUBLISHED as a terminal-but-reversible state and DELETED as soft-delete. Staff create/edit in DRAFT; submitting moves to IN_REVIEW. Reviewers approve to PUBLISHED or bounce back to DRAFT with a required reason. Required fields (event_date MM/DD/YYYY, event_time 12-hour AM/PM, location_text ≤200 chars, single category, 0–10 tags of 2–24 chars) are validated at the DRAFT→IN_REVIEW boundary. All transitions logged in audit_logs with actor, timestamp (local + offset), and reason.

---

### 2. How are attachment uploads and deduplication enforced?
- **Problem:** The prompt caps attachments at 10 files / 25 MB total, requires 1920px long-edge compression, and mandates SHA-256 fingerprints for dedup — but does not define dedup scope or behavior on collision.
- **My Understanding:** Dedup is per-facility at the storage layer; duplicates are referenced rather than re-stored.
- **Solution:** Before persisting, compute SHA-256 of the compressed bytes. If a row with the same fingerprint exists within the same facility, create a new `attachments` record pointing to the existing blob path (reference-count the blob) instead of writing a duplicate file. Images are resized locally to max 1920px long edge before hashing so re-uploads of the same source dedup consistently. Aggregate size (sum of compressed bytes) is enforced at ≤25 MB per parent record; exceeding either the 10-file or 25 MB cap returns a 413 with which limit was hit.

---

### 3. How does the asset lifecycle state machine work?
- **Problem:** The prompt lists eight states (intake, assignment, loan, transfer, maintenance, repair, inventory_count, disposal) without defining allowed transitions or bulk-action semantics.
- **My Understanding:** Not every state can reach every other state; bulk actions must all target the same destination and fail atomically per row.
- **Solution:** Allowed transitions — INTAKE → ASSIGNMENT | INVENTORY_COUNT; ASSIGNMENT → LOAN | TRANSFER | MAINTENANCE | INVENTORY_COUNT; LOAN → ASSIGNMENT | MAINTENANCE; MAINTENANCE → REPAIR | ASSIGNMENT; REPAIR → ASSIGNMENT | DISPOSAL; TRANSFER → ASSIGNMENT (at destination facility); INVENTORY_COUNT → prior state; DISPOSAL is terminal. Bulk actions accept ≤500 asset IDs and exactly one target state; each row is validated against the transition table — invalid rows are reported per-ID in the response, valid rows commit in a single transaction. Barcode/QR label uniqueness is enforced by a composite unique index on (facility_id, asset_label).

---

### 4. How do photography package variants and inventory linkage work?
- **Problem:** Packages support up to 20 variant combinations, USD pricing with two decimals, included items, and optional links to internal inventory/time slots — but variant pricing rules and slot reservation semantics are not defined.
- **My Understanding:** Variants are axis combinations (e.g., size × finish); each combination has its own price and optional inventory/slot binding.
- **Solution:** `packages` holds base price and publish flag; `package_variants` holds up to 20 rows per package, each with a distinct combination key, price (NUMERIC(10,2), ≥0), optional `inventory_item_id`, and optional `time_slot_id`. Publishing validates that every variant has a price and that any linked inventory item or slot exists within the same facility. Unpublish is reversible and does not delete variants. Time slots are internal records only — no external calendar sync.

---

### 5. How does the offline notification outbox and retry policy work?
- **Problem:** The system must queue email/SMS/webhook deliveries offline with templating, up to 3 retries, and per-user subscription preferences — but retry timing, export format, and template variable scope are not defined.
- **My Understanding:** Outbox rows are generated at trigger events, rendered from a whitelisted field set, and exportable as a structured batch for an external sender to process.
- **Solution:** Triggers — submission, supplement, review, change — write to `outbox_deliveries` with channel, recipient, rendered body, template_id, variables snapshot, status (PENDING → SENT | FAILED), and attempt_count. Templates render only from an approved field allowlist (no arbitrary SQL/object access). Retries on FAILED occur up to 3 times with exponential backoff (1m, 5m, 30m); after 3 failures status becomes DEAD and surfaces in admin. Export endpoint returns PENDING rows as JSON Lines for an offline mailer; on re-import, matching rows are marked SENT with send-log entries. Per-user subscription preferences are checked at enqueue time; opted-out recipients skip enqueue entirely.

---

### 6. How do RBAC data scope, field-level masking, and idempotency interact?
- **Problem:** RBAC spans permission-to-resource mapping, single-facility vs all-facilities scope, and field-level access; separately, all writes must be idempotent via a client request_id (UUID) retained 24 hours per user. Interaction between masked reads and replayed writes is not defined.
- **My Understanding:** Scope and field access are evaluated per-request; idempotency keys cache the full response the caller was authorized to see.
- **Solution:** Each role carries (a) resource permissions, (b) data scope (`facility:<id>` or `facility:*`), and (c) a field allowlist. Queries filter by facility_id when scope is single-facility; sensitive fields (gov ID, certificates, private volunteer notes) are encrypted at rest (AES-GCM with a local KEK) and returned masked (last-4 only) unless the caller's field allowlist grants full view. For idempotency, the first write with a given (user_id, request_id) commits and the response is stored in `idempotency_keys` for 24 hours; replays return the cached response verbatim — including whatever masking applied to the original caller, so a different user replaying the same key is rejected with 409. Authentication is local only: ≥12-char salted-hashed passwords, lockout after 5 failed attempts in 15 minutes, 8-hour idle session expiry.

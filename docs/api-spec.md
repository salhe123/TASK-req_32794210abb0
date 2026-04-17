# CivicOps Operations & Services Management - API Specification

Base URL: `/api`
All write endpoints require header `X-Request-Id: <UUID>` for idempotency (retained 24 hours per user).
All authenticated endpoints require header `Authorization: Bearer <session-token>`.
All list endpoints support `page` and `size` query parameters.

---

## 1. Authentication & Session (`/api/auth`)

### POST `/login`
Request:
```json
{ "username": "desk1", "password": "MinimumTwelveCharsPass" }
```
Response:
```json
{ "token": "opaque-session-token", "expiresInSeconds": 28800, "role": "DESK_STAFF" }
```
Errors:
- 401 `unauthenticated` — invalid credentials
- 423 `account_locked` — 5 failed attempts within 15 minutes

### POST `/logout`
Invalidates the current session.

### POST `/change-password`
```json
{ "oldPassword": "...", "newPassword": "AtLeastTwelveChars" }
```
Errors:
- 400 `validation_failed` — new password < 12 chars

### GET `/session`
Returns current session metadata and remaining idle time.

---

## 2. Lost-and-Found (`/api/lost-found`)  *(addresses Q1, Q2)*

### POST `/items`
Creates an item in `DRAFT`.
```json
{
  "facilityId": "uuid",
  "category": "ELECTRONICS",
  "tags": ["phone", "black"],
  "eventDate": "04/17/2026",
  "eventTime": "02:30 PM",
  "locationText": "Main lobby near east entrance"
}
```
Validation:
- `locationText` ≤ 200 chars
- `tags`: 0–10 entries, each 2–24 chars
- `eventDate` MM/DD/YYYY, `eventTime` 12-hour with AM/PM

### GET `/items`
Filters: `facilityId`, `status`, `category`, `tag`, `keyword`, `dateFrom`, `dateTo`.

### GET `/items/{id}`

### PUT `/items/{id}`
Editable only in `DRAFT`.

### POST `/items/{id}/submit`
Transition `DRAFT → IN_REVIEW`. Validates all required fields.
Errors:
- 409 `invalid_transition`

### POST `/items/{id}/approve`
DESK_REVIEWER only. Transition `IN_REVIEW → PUBLISHED`.

### POST `/items/{id}/bounce`
```json
{ "reason": "photo too dark, please replace" }
```
Transition `IN_REVIEW → DRAFT`.

### POST `/items/{id}/unpublish`
Transition `PUBLISHED → UNPUBLISHED` (reversible).

### POST `/items/{id}/republish`
Transition `UNPUBLISHED → PUBLISHED`.

### DELETE `/items/{id}`
Soft delete.

### GET `/items/{id}/history`
Returns immutable transition history.

### POST `/items/{id}/attachments`
Multipart upload. Up to 10 files / 25 MB aggregate per item. Accepted: JPEG/PNG/WebP/PDF. Images are compressed locally to max 1920 px long edge and deduplicated by SHA-256 within the facility.
Response:
```json
{
  "attachmentId": "uuid",
  "sha256": "hex",
  "sizeBytes": 184320,
  "deduplicated": true
}
```
Errors:
- 413 `attachment_limit_exceeded` — `{ "limit": "files" | "bytes" }`
- 400 `invalid_attachment` — wrong MIME

### GET `/items/{id}/attachments`

### DELETE `/items/{id}/attachments/{attachmentId}`
Decrements the blob's reference count; storage is reclaimed when `ref_count = 0`.

---

## 3. Asset Lifecycle (`/api/assets`)  *(addresses Q3)*

### POST `/`
```json
{ "facilityId": "uuid", "assetLabel": "QR-00042", "initialState": "INTAKE" }
```
Errors:
- 409 `duplicate_asset_label` — label already exists in facility

### GET `/`
Filters: `facilityId`, `state`, `assetLabel`, `createdFrom`, `createdTo`.

### GET `/{id}`

### GET `/{id}/history`
Returns `asset_events` (immutable).

### POST `/{id}/transition`
```json
{ "toState": "ASSIGNMENT", "reason": "assigned to maintenance team" }
```
Validated against the transition table. Errors:
- 409 `invalid_transition` — `{ "from": "INTAKE", "to": "LOAN" }`

### POST `/bulk-transition`
```json
{
  "assetIds": ["uuid", "uuid", "..."],
  "toState": "INVENTORY_COUNT",
  "reason": "Q2 count"
}
```
- Max 500 IDs
- Exactly one `toState`
- Single DB transaction; per-ID validation results returned

Response:
```json
{
  "committed": 487,
  "rejected": [
    { "assetId": "uuid", "error": "invalid_transition", "from": "DISPOSAL" }
  ]
}
```

### POST `/{id}/maintenance-records`
```json
{ "technicianUserId": "uuid", "notes": "replaced battery" }
```

### GET `/{id}/maintenance-records`

---

## 4. Volunteer Qualification (`/api/volunteers`)

### POST `/`
```json
{
  "facilityId": "uuid",
  "displayName": "Jordan P.",
  "govId": "123456789",
  "privateNotes": "allergic to shellfish"
}
```
Sensitive fields are encrypted at rest; responses return masked values (e.g., `"govId": "*****6789"`) unless the caller's role has full-field access.

### GET `/`
Filters: `facilityId`, `qualificationExpiringBefore`.

### GET `/{id}`

### PUT `/{id}`

### DELETE `/{id}`

### POST `/{id}/qualifications`
```json
{
  "name": "First Aid",
  "issuer": "Red Cross",
  "certificate": "RC-2025-0099",
  "issuedOn": "03/01/2025",
  "expiresOn": "03/01/2027"
}
```

### GET `/{id}/qualifications`

### DELETE `/{id}/qualifications/{qualificationId}`

---

## 5. Photography Packages (`/api/packages`)  *(addresses Q4)*

### POST `/`
```json
{
  "facilityId": "uuid",
  "name": "Family Portrait Basic",
  "basePrice": "120.00",
  "includedItems": ["10 prints 4x6", "1 digital copy"]
}
```
`basePrice` is USD, two-decimal string to avoid float drift.

### GET `/`
Filters: `facilityId`, `isPublished`, `keyword`.

### GET `/{id}`

### PUT `/{id}`

### DELETE `/{id}`

### POST `/{id}/publish`
Validates: every variant has a price; any linked `inventoryItemId` or `timeSlotId` exists in the same facility.

### POST `/{id}/unpublish`

### POST `/{id}/variants`
```json
{
  "combinationKey": "size=8x10;finish=matte",
  "price": "35.00",
  "inventoryItemId": null,
  "timeSlotId": null
}
```
Errors:
- 400 `validation_failed` — >20 variants per package, negative price, or duplicate combinationKey

### GET `/{id}/variants`

### PUT `/{id}/variants/{variantId}`

### DELETE `/{id}/variants/{variantId}`

---

## 6. Notification Center (`/api/notifications`)  *(addresses Q5)*

### GET `/inbox`
Returns in-app notifications for the authenticated user. Filters: `readState` (`unread`|`read`|`all`).

### POST `/inbox/{id}/read`
Marks a single notification read.

### POST `/inbox/mark-all-read`

### GET `/templates`
Admin-only.

### POST `/templates`
```json
{
  "name": "item_submitted",
  "channel": "IN_APP",
  "triggerEvent": "submission",
  "subject": "Your item was submitted",
  "body": "Item {{item.id}} is now in review."
}
```
Templates render only from an approved variable allowlist (enforced at render).

### PUT `/templates/{id}`

### DELETE `/templates/{id}`

### GET `/outbox`
Filters: `status` (`PENDING`|`SENT`|`FAILED`|`DEAD`), `channel` (`EMAIL`|`SMS`|`WEBHOOK`), `createdFrom`, `createdTo`.

### GET `/outbox/export`
Streams PENDING outbox rows as JSON Lines for offline dispatch:
```
{"id":"uuid","channel":"EMAIL","recipient":"...","subject":"...","body":"...","attempt":1}
```

### POST `/outbox/import-results`
Bulk-acknowledge exported deliveries.
```json
{
  "results": [
    { "id": "uuid", "status": "SENT", "sentAt": "04/17/2026 10:15 AM" },
    { "id": "uuid", "status": "FAILED", "error": "SMTP 550 no such user" }
  ]
}
```
Retry policy: up to 3 attempts with backoff 1 min / 5 min / 30 min; 4th failure → `DEAD`.

### GET `/subscriptions`
Returns the caller's per-event channel preferences.

### PUT `/subscriptions`
```json
{
  "submission": { "inApp": true, "email": false },
  "review":     { "inApp": true, "email": true }
}
```

---

## 7. Administration & RBAC (`/api/admin`)  *(addresses Q6)*

### Users — `/admin/users`

#### POST `/`
```json
{ "username": "reviewer1", "password": "AtLeastTwelveChars", "roleIds": ["uuid"] }
```

#### GET `/`
Filters: `role`, `status`, `keyword`.

#### PUT `/{id}`
Update username / role assignments / active flag.

#### PUT `/{id}/unlock`
Clears failed-attempt lockout.

#### POST `/{id}/reset-password`
Admin-assisted reset; new password must meet policy.

### Roles — `/admin/roles`

#### POST `/`
```json
{
  "name": "DESK_REVIEWER",
  "dataScope": "facility:uuid-or-*",
  "fieldAllowlist": ["volunteers.govId"],
  "permissions": [
    { "resource": "lost_found_items", "action": "approve" }
  ]
}
```

#### GET `/`

#### PUT `/{id}`

#### DELETE `/{id}`

### Permissions — `/admin/permissions`

#### GET `/`
Returns the full resource/action catalog.

### Facilities — `/admin/facilities`

#### POST `/`, GET `/`, PUT `/{id}`, DELETE `/{id}`

### Audit Logs — `/admin/audit`

#### GET `/logs`
Append-only — no mutation endpoints.
Filters: `actorUserId`, `resourceType`, `resourceId`, `action`, `facilityId`, `dateFrom`, `dateTo`.

### Idempotency Inspection — `/admin/idempotency`

#### GET `/keys`
Filters: `userId`, `status`, `createdFrom`.
Admin-only — used to debug replays.
Records older than 24 hours are reaped automatically and not returned.

---

## 8. Health & Diagnostics

### GET `/health`
Process liveness. Returns `200 { "status": "ok" }`.

### GET `/health/ready`
Checks DB connectivity and that migrations are applied.

### GET `/metrics`
Basic counters for offline incident review:
```json
{
  "requestsTotal": 18423,
  "errorsTotal": 12,
  "outboxPending": 4,
  "outboxDead": 0,
  "activeSessions": 17
}
```

---

## 9. Conventions

- **Idempotency (Q6):** `X-Request-Id: <UUID>` is required on every POST/PUT/DELETE. Replays return the originally-stored response verbatim. A different user replaying someone else's `request_id` gets `409 idempotency_conflict`. Keys expire after 24 hours.
- **RBAC scope & field masking (Q6):** list endpoints are automatically filtered by the caller's data scope; sensitive fields return masked values unless granted by the caller's `fieldAllowlist`.
- **Timestamps:** responses use MM/DD/YYYY for dates and 12-hour AM/PM for times, with each record also carrying the originating UTC offset.
- **Money:** all USD prices are two-decimal strings (e.g., `"35.00"`).
- **Errors:** always envelope `{ "error": "<code>", "message": "...", "details": { ... } }`.

| HTTP | Common codes |
|------|--------------|
| 400 | `validation_failed`, `invalid_attachment` |
| 401 | `unauthenticated`, `session_expired` |
| 403 | `forbidden`, `out_of_scope` |
| 404 | `not_found` |
| 409 | `invalid_transition`, `duplicate_asset_label`, `idempotency_conflict` |
| 413 | `attachment_limit_exceeded` |
| 423 | `account_locked` |
| 429 | `rate_limited` |
| 500 | `internal_error` |

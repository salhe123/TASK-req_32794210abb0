# CivicOps

Offline-capable backend for lost-and-found, asset lifecycle, volunteer
qualifications, photography packages, and local notification outbox.

## Run

```
cp .env.example .env
docker compose up --build
```

The service binds `:8080`. A bootstrap admin user is created on first start:

```
username: admin
password: ChangeMeSoon1234
```

Change the password via `POST /api/auth/change-password` after first login.

## Environment

| var | default | description |
|-----|---------|-------------|
| `DATABASE_URL` | — | Postgres connection string |
| `BIND_ADDR` | `0.0.0.0:8080` | HTTP bind |
| `SESSION_TTL_SECS` | `28800` | Session absolute TTL (8h) |
| `KEK_PATH` | `/var/civicops/kek.bin` | AES-256 KEK file; generated on first boot |
| `BLOB_DIR` | `/var/civicops/blobs` | Attachment blob store |
| `OUTBOX_EXPORT_DIR` | `/var/civicops/outbox` | Outbox JSONL export destination |
| `RUST_LOG` | `info` | Tracing filter |
| `SEED_TEST_FIXTURES` | `false` | Seed test users/roles (test harness only) |
| `CIVICOPS_ENABLE_DIAG` | `false` | Mount `/api/__diag/*` endpoints (test harness only) |
| `CIVICOPS_LOCAL_OFFSET_MINUTES` | host local | Offset (in minutes) attached to every `*_offset_minutes` column written by the service. Accepts values in `[-1440, 1440]`. |

## Tests

```
./run_tests.sh
```

Runs the full unit + API + coverage pipeline inside Docker. The script
builds the service image, starts a disposable Postgres, runs migrations,
executes `cargo test`, then enforces ≥ 90 % coverage via `cargo tarpaulin`.
Coverage enforcement is on by default; set `CIVICOPS_SKIP_COVERAGE=1` to
skip it in constrained environments. Set `CIVICOPS_RUN_LOAD=1` to also
execute the concurrent-load test.

## Endpoints

- `/api/auth` — local login, session, password change
- `/api/lost-found` — intake, review workflow, attachments (SHA-256 dedup)
- `/api/assets` — state machine, bulk transitions, maintenance
- `/api/volunteers` — profile + qualifications (encrypted sensitive fields)
- `/api/packages` — photography packages and per-facility variants
- `/api/notifications` — inbox, templates, outbox export/import
- `/api/admin` — users, roles, permissions, facilities, audit
- `/health`, `/api/health`, `/api/health/ready`, `/api/metrics`

Outbox exports land in `OUTBOX_EXPORT_DIR` when the export endpoint writes
to disk; otherwise the endpoint streams JSONL directly.

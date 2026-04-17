# CivicOps

An offline-capable backend service for a local civic organization. It covers
lost-and-found intake/review, the full asset lifecycle state machine, volunteer
qualifications with encrypted sensitive fields, photography packages with
per-facility variants, a notification inbox with outbox export/import for
external relays (email/SMS/webhook), and an administrative surface for users,
roles, permissions, facilities, and audit logs.

## Architecture & Tech Stack

* **Frontend:** *Not applicable — backend-only service.*
* **Backend:** Rust (edition 2021), Actix-web 4, Diesel 2.2 ORM, r2d2 connection pool,
  argon2 password hashing, AES-256-GCM field encryption, `tracing` JSON logs.
* **Database:** PostgreSQL 16
* **Containerization:** Docker & Docker Compose (Required)

## Project Structure

```text
.
├── src/                    # Rust backend source
│   ├── handlers/           # Actix route handlers, grouped by domain
│   ├── middleware/         # Auth, idempotency, access-log, request context
│   ├── models/             # Diesel models (one per table group)
│   ├── services/           # Cross-cutting services (crypto, session, notify, …)
│   ├── schema.rs           # Diesel schema mirroring migrations/
│   ├── errors.rs           # AppError + JSON envelope + redaction
│   ├── config.rs           # Env-driven Config
│   ├── db.rs               # r2d2 Postgres pool
│   ├── metrics.rs          # Request/error counters
│   └── main.rs             # Binary entry point
├── migrations/             # Sequential Diesel migrations (000001 … 000010)
├── tests/                  # Integration + API tests (reqwest-driven)
│   └── common/mod.rs       # Shared test helpers
├── Cargo.toml              # Rust manifest
├── Dockerfile              # Service image (builder + slim runtime)
├── .env.example            # Example environment variables
├── docker-compose.yml      # Multi-container orchestration - MANDATORY
├── run_tests.sh            # Standardized test execution script - MANDATORY
└── README.md               # Project documentation - MANDATORY
```

## Prerequisites

To ensure a consistent environment, this project is designed to run entirely
within containers. You must have the following installed:

* [Docker](https://docs.docker.com/get-docker/)
* [Docker Compose](https://docs.docker.com/compose/install/)

## Running the Application

1. **Seed the environment file (optional — `docker compose` reads sensible defaults):**
   ```bash
   cp .env.example .env
   ```

2. **Build and Start Containers:**
   Use Docker Compose to build the images and spin up the entire stack in detached mode.
   ```bash
   docker-compose up --build -d
   ```
   On first boot the service generates the AES-256 KEK under `KEK_PATH`, runs
   all Diesel migrations, and seeds the bootstrap admin user.

3. **Access the App:**
   * Backend API: `http://localhost:8080/api`
   * Liveness probe: `http://localhost:8080/health`
   * Readiness probe: `http://localhost:8080/api/health/ready`
   * Metrics: `http://localhost:8080/api/metrics`
   * API Documentation: *not shipped — see the endpoint summary below.*

4. **Stop the Application:**
   ```bash
   docker-compose down -v
   ```

### API Surface

| Prefix | Responsibilities |
| :--- | :--- |
| `/api/auth` | Login, logout, session, password change (X-Request-Id required) |
| `/api/lost-found` | Intake, review workflow, SHA-256-deduplicated attachments |
| `/api/assets` | State machine, bulk transitions, maintenance records |
| `/api/volunteers` | Profile + qualifications (encrypted gov ID / notes / certificate) |
| `/api/packages` | Photography packages, variants, included items |
| `/api/notifications` | Inbox, templates, outbox export/import, dispatch, subscriptions |
| `/api/admin` | Users, roles, permissions, facilities, audit logs, idempotency keys |
| `/health`, `/api/health`, `/api/health/ready`, `/api/metrics` | Operational endpoints |

Outbox exports are streamed as NDJSON and also persisted to
`OUTBOX_EXPORT_DIR` as timestamped snapshots so an external relay has a durable
file record of what was shipped.

### Environment variables

| var | default | description |
| :--- | :--- | :--- |
| `DATABASE_URL` | — | Postgres connection string |
| `BIND_ADDR` | `0.0.0.0:8080` | HTTP bind |
| `SESSION_TTL_SECS` | `28800` | Session absolute TTL (8h) |
| `KEK_PATH` | `/var/civicops/kek.bin` | AES-256 KEK file; generated on first boot |
| `BLOB_DIR` | `/var/civicops/blobs` | Attachment blob store |
| `OUTBOX_EXPORT_DIR` | `/var/civicops/outbox` | Outbox NDJSON export destination |
| `RUST_LOG` | `info` | Tracing filter |
| `SEED_TEST_FIXTURES` | `false` | Seed test users/roles (test harness only) |
| `CIVICOPS_ENABLE_DIAG` | `false` | Mount `/api/__diag/*` endpoints (test harness only) |
| `CIVICOPS_LOCAL_OFFSET_MINUTES` | host local | Offset (in minutes) attached to every `*_offset_minutes` column written by the service. Accepts values in `[-1440, 1440]`. |

## Testing

All unit, integration, and E2E tests are executed via a single, standardized
shell script. This script automatically handles any necessary container
orchestration for the test environment.

Make sure the script is executable, then run it:

```bash
chmod +x run_tests.sh
./run_tests.sh
```

The script builds the service image, starts a disposable Postgres on a private
Docker network, runs migrations, executes `cargo test` inside the test
container, exercises a DB-down readiness assertion, and finally enforces
≥ 90 % coverage via `cargo tarpaulin`. Coverage enforcement is on by default;
set `CIVICOPS_SKIP_COVERAGE=1` to skip it in constrained environments. Set
`CIVICOPS_RUN_LOAD=1` to additionally execute the concurrent-load test.

*Note: The `run_tests.sh` script outputs a standard exit code (`0` for success,
non-zero for failure) to integrate smoothly with CI/CD validators.*

## Seeded Credentials

The database is pre-seeded with the following users on startup. CivicOps
authenticates by **username**, not by email — each call to `POST /api/auth/login`
must also supply an `X-Request-Id` header (any UUID) to satisfy the idempotency
contract. All test users below require `SEED_TEST_FIXTURES=true`; only the
bootstrap admin is created unconditionally.

| Role | Username | Password | Notes |
| :--- | :--- | :--- | :--- |
| **Bootstrap Admin** | `admin` | `ChangeMeSoon1234` | SYSTEM_ADMIN; change via `POST /api/auth/change-password` after first login. |
| **System Admin** | `test_admin` | `TestAdminPassword123` | Full access to every module and all facilities. |
| **Desk Staff** | `test_desk` | `TestDeskPassword123` | Creates/edits DRAFT lost-and-found items. |
| **Desk Reviewer** | `test_review` | `TestReviewPassword123` | Approves or bounces lost-and-found submissions. |
| **Asset Manager** | `test_asset` | `TestAssetPassword123` | Creates assets and runs state-machine transitions. |
| **Volunteer Coordinator** | `test_vol` | `TestVolPassword123` | Manages volunteer records (no access to sensitive fields). |
| **Volunteer Admin** | `test_vol_full` | `TestVolFullPassword123` | Above + read/write access to gov ID, private notes, certificates. |
| **Package Manager** | `test_pkg` | `TestPkgPassword123` | Manages photography packages and variants. |
| **Notification Admin** | `test_notif` | `TestNotifPassword123` | Manages templates, outbox, and multi-channel dispatch. |
| **Scoped Desk (Other Facility)** | `test_other` | `TestOtherPassword123` | Desk-staff limited to the `SECONDARY` facility (used for scope tests). |

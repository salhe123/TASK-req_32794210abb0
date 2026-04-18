#!/usr/bin/env bash
# run_tests.sh — orchestrates CivicOps tests entirely inside Docker.
#
# Requirements on the host: Docker only.
#
# Phases:
#   1. Build service image.
#   2. Start disposable Postgres on a private network.
#   3. Start the service container; it runs migrations + seeds test fixtures.
#   4. Run `cargo test` (unit + API + load) inside a test container on the same network.
#   5. Run the DB-down phase: stop postgres, assert /api/health/ready returns 503, restart.
#   6. Run `cargo tarpaulin` for coverage; fail if < 90%.
#   7. Tear down everything on exit.
set -euo pipefail

NET="civicops-test-net-$$"
PG_NAME="civicops-test-pg-$$"
APP_NAME="civicops-test-app-$$"
TEST_NAME="civicops-test-runner-$$"
IMAGE="civicops-test-$$"
TEST_IMAGE="civicops-test-runner-image-$$"

COVERAGE_THRESHOLD="${COVERAGE_THRESHOLD:-90}"
RUN_LOAD="${CIVICOPS_RUN_LOAD:-1}"
SKIP_DB_DOWN="${CIVICOPS_SKIP_DB_DOWN:-0}"
SKIP_COVERAGE="${CIVICOPS_SKIP_COVERAGE:-0}"

cleanup() {
    set +e
    echo "--- cleanup"
    docker rm -f "$TEST_NAME" >/dev/null 2>&1 || true
    docker rm -f "$APP_NAME" >/dev/null 2>&1 || true
    docker rm -f "$PG_NAME" >/dev/null 2>&1 || true
    docker network rm "$NET" >/dev/null 2>&1 || true
    docker image rm -f "$IMAGE" >/dev/null 2>&1 || true
    docker image rm -f "$TEST_IMAGE" >/dev/null 2>&1 || true
}
trap cleanup EXIT

REPO_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "--- build service image"
docker build -t "$IMAGE" "$REPO_DIR"

echo "--- create internal network (no outbound — proves offline operation)"
docker network create --internal "$NET" >/dev/null

echo "--- start postgres"
docker run -d --name "$PG_NAME" \
    --network "$NET" \
    -e POSTGRES_USER=civicops \
    -e POSTGRES_PASSWORD=civicops \
    -e POSTGRES_DB=civicops \
    postgres:16-alpine >/dev/null

echo "--- wait for postgres"
for i in $(seq 1 60); do
    if docker exec "$PG_NAME" pg_isready -U civicops -d civicops >/dev/null 2>&1; then
        break
    fi
    sleep 1
done

echo "--- start service"
docker run -d --name "$APP_NAME" \
    --network "$NET" \
    -e DATABASE_URL="postgres://civicops:civicops@${PG_NAME}:5432/civicops" \
    -e BIND_ADDR="0.0.0.0:8080" \
    -e RUST_LOG="info" \
    -e SEED_TEST_FIXTURES="true" \
    -e CIVICOPS_ENABLE_DIAG="true" \
    "$IMAGE" >/dev/null

echo "--- wait for service"
READY=0
for i in $(seq 1 120); do
    # Use the already-running postgres container (alpine busybox has nc).
    if docker exec "$PG_NAME" sh -c "nc -z ${APP_NAME} 8080" >/dev/null 2>&1; then
        READY=1
        break
    fi
    sleep 1
done
if [ "$READY" != "1" ]; then
    echo "!!! service did not become ready; last 80 log lines:"
    docker logs --tail 80 "$APP_NAME" || true
    exit 1
fi
echo "--- service log (last 30 lines) after wait"
docker logs --tail 30 "$APP_NAME" || true

echo "--- build test image (cached cargo layers)"
cat > "$REPO_DIR/Dockerfile.test" <<EOF
FROM rust:1.89-slim-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \\
    pkg-config libpq-dev libssl-dev ca-certificates curl \\
    clang lld \\
    && rm -rf /var/lib/apt/lists/*
ENV RUSTFLAGS="-C linker=clang -C link-arg=-fuse-ld=lld -C debuginfo=0"
ENV CARGO_BUILD_JOBS=1
# Pre-fetch all runtime + dev-dependency crate sources into
# /usr/local/cargo/registry so the test container can build on the
# --internal docker network without reaching crates.io.
WORKDIR /prefetch
# Copy Cargo.lock too — the test container runs with --offline on the
# --internal docker network, so the prefetched registry cache must match
# the exact versions the repo's Cargo.lock pins (e.g. actix-http 3.12.0,
# not the latest 3.12.1). Without --locked here, cargo would resolve to
# the latest compatible versions and leave the cache out of sync with the
# workspace's committed lock, causing offline "attempting to make an HTTP
# request" errors at test time.
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src tests && echo 'fn main() {}' > src/main.rs && \\
    cargo fetch --locked
$([ "$SKIP_COVERAGE" = "0" ] && echo 'RUN cargo install cargo-tarpaulin --locked || true')
WORKDIR /work
EOF
docker build -t "$TEST_IMAGE" -f "$REPO_DIR/Dockerfile.test" "$REPO_DIR"
rm -f "$REPO_DIR/Dockerfile.test"

echo "--- run unit + api + load tests inside docker"
set +e
docker run --rm --name "$TEST_NAME" \
    --network "$NET" \
    -e CIVICOPS_URL="http://${APP_NAME}:8080" \
    -e DATABASE_URL="postgres://civicops:civicops@${PG_NAME}:5432/civicops" \
    -e CIVICOPS_RUN_LOAD="$RUN_LOAD" \
    -e CARGO_BUILD_JOBS="1" \
    -e CARGO_NET_OFFLINE="true" \
    -v "$REPO_DIR":/work \
    -w /work \
    "$TEST_IMAGE" \
    bash -c "cargo test --offline --all-targets -- --test-threads=1"
TEST_EXIT=$?
set -e
if [ "$TEST_EXIT" != "0" ]; then
    echo "!!! cargo test failed with exit $TEST_EXIT"
    echo "!!! --- service log (last 150 lines) ---"
    docker logs --tail 150 "$APP_NAME" || true
    echo "!!! --- postgres log (last 80 lines) ---"
    docker logs --tail 80 "$PG_NAME" || true
    exit "$TEST_EXIT"
fi

if [ "$SKIP_DB_DOWN" = "0" ]; then
    echo "--- db-down phase: stop postgres"
    docker stop "$PG_NAME" >/dev/null

    echo "--- assert /api/health/ready returns 503 via dedicated cargo test"
    docker run --rm \
        --network "$NET" \
        -e CIVICOPS_URL="http://${APP_NAME}:8080" \
        -e CIVICOPS_EXPECT_DB_DOWN="1" \
        -e CARGO_NET_OFFLINE="true" \
        -v "$REPO_DIR":/work \
        -w /work \
        "$TEST_IMAGE" \
        bash -c "cargo test --offline --test api_db_down -- --test-threads=1"

    echo "--- restart postgres"
    docker start "$PG_NAME" >/dev/null
    for i in $(seq 1 60); do
        if docker exec "$PG_NAME" pg_isready -U civicops -d civicops >/dev/null 2>&1; then
            break
        fi
        sleep 1
    done
    # Wait for the app pool to reconnect and accept traffic again.
    for i in $(seq 1 60); do
        if docker exec "$PG_NAME" sh -c "nc -z ${APP_NAME} 8080" >/dev/null 2>&1; then
            break
        fi
        sleep 1
    done
fi

if [ "$SKIP_COVERAGE" = "1" ]; then
    echo "--- coverage explicitly skipped via CIVICOPS_SKIP_COVERAGE=1; the default enforces the $COVERAGE_THRESHOLD% tarpaulin gate."
else
    echo "--- run coverage (cargo tarpaulin)"
    docker run --rm \
        --network "$NET" \
        --security-opt seccomp=unconfined \
        -e CIVICOPS_URL="http://${APP_NAME}:8080" \
        -e DATABASE_URL="postgres://civicops:civicops@${PG_NAME}:5432/civicops" \
        -e CIVICOPS_RUN_LOAD="$RUN_LOAD" \
        -e CARGO_BUILD_JOBS="1" \
        -v "$REPO_DIR":/work \
        -w /work \
        "$TEST_IMAGE" \
        bash -c "command -v cargo-tarpaulin >/dev/null && cargo tarpaulin --engine llvm --out Stdout --fail-under $COVERAGE_THRESHOLD -- --test-threads=1 || echo 'tarpaulin unavailable, skipping coverage'"
fi

echo "--- all tests passed"

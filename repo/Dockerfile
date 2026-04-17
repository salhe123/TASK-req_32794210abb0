FROM rust:1.89-slim-bookworm AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libpq-dev libssl-dev ca-certificates \
    clang lld \
    && rm -rf /var/lib/apt/lists/*
# Cap concurrent compile jobs so the builder doesn't hit OOM on memory-
# constrained hosts (Docker Desktop defaults ~4 GB, and diesel/actix-web
# are memory-heavy to compile). Use lld as the final linker because the
# GNU linker keeps every rlib's symbol table in memory at once.
ENV CARGO_BUILD_JOBS=1
ENV CARGO_NET_RETRY=10
ENV RUSTFLAGS="-C linker=clang -C link-arg=-fuse-ld=lld -C debuginfo=0"
# Build profile is configurable. "debug" compiles reliably on ~4 GB hosts;
# "release" produces a faster binary but needs more build-time memory.
ARG PROFILE=debug
ENV PROFILE=${PROFILE}
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
COPY migrations ./migrations
COPY diesel.toml ./diesel.toml
RUN if [ "$PROFILE" = "release" ]; then \
        cargo build --release --locked || cargo build --release ; \
        cp target/release/civicops /usr/local/bin/civicops ; \
    else \
        cargo build --locked || cargo build ; \
        cp target/debug/civicops /usr/local/bin/civicops ; \
    fi

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    libpq5 ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home civicops
WORKDIR /app
COPY --from=builder /usr/local/bin/civicops /app/civicops
COPY --from=builder /build/migrations /app/migrations
RUN mkdir -p /var/civicops && chown -R civicops:civicops /var/civicops /app
USER civicops
EXPOSE 8080
CMD ["/app/civicops"]

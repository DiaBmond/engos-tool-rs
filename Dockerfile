# syntax=docker/dockerfile:1

# ---------------------------------------------------------------------------
# Build stage
#
# SQLX_OFFLINE makes `sqlx::query!` validate against the committed `.sqlx/`
# metadata instead of opening a database connection, so the image builds with
# no database reachable.
# ---------------------------------------------------------------------------
FROM rust:1.96-slim-bookworm AS builder

WORKDIR /app
ENV SQLX_OFFLINE=true

RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Warm the dependency cache separately from the source, so editing application
# code does not rebuild every crate.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src \
    && echo 'fn main() {}' > src/main.rs \
    && echo '' > src/lib.rs \
    && cargo build --release \
    && rm -rf src

COPY .sqlx ./.sqlx
COPY src ./src

# Cargo skips rebuilding when mtimes look unchanged after the dummy build.
RUN touch src/main.rs src/lib.rs && cargo build --release

# ---------------------------------------------------------------------------
# Runtime stage
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 engos

WORKDIR /app
COPY --from=builder /app/target/release/engos-tool-rs /usr/local/bin/engos

# Never run as root; the process needs no write access to the filesystem.
USER engos

ENV HOST=0.0.0.0 \
    PORT=8080 \
    LOG_FORMAT=json \
    RUST_LOG=engos_tool_rs=info,tower_http=warn,sqlx=warn

EXPOSE 8080

# `/healthz` is dependency-free, so a database blip cannot restart the container.
# Orchestrators should additionally probe `/readyz`, which does check both stores.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS "http://127.0.0.1:${PORT}/healthz" || exit 1

ENTRYPOINT ["/usr/local/bin/engos"]

# ── Build stage ───────────────────────────────────────────────────────────────
FROM rust:1.88-bookworm AS builder
WORKDIR /build

# Cache dependency compilation separately from source changes
COPY Cargo.toml Cargo.lock build.rs ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release \
    && rm -f target/release/deps/backend*

COPY src ./src
RUN cargo build --release

# ── Runtime stage ─────────────────────────────────────────────────────────────
FROM debian:bookworm-slim
# curl is here for the container healthcheck, which asks /health what it says
# rather than only whether it answers. The image carried no HTTP client at all,
# so the alternative was a bare TCP probe, and a port that accepts connections
# would not have caught any failure this service has actually had.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /build/target/release/backend .

EXPOSE 3000
CMD ["./backend"]

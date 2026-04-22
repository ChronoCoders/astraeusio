# ── Build stage ───────────────────────────────────────────────────────────────
FROM rust:1.82-bookworm AS builder
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
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /build/target/release/backend .

EXPOSE 3000
CMD ["./backend"]

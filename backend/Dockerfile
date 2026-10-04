# ── Build stage ───────────────────────────────────────────────────────────────
# The tag fixes the Debian release and nothing else. bookworm has to match the
# runtime stage below, because the binary is copied across and links against
# that glibc. The Rust version is not written here: rust-toolchain.toml is
# copied in below and rustup honours it, so this build and the gate compile with
# the same compiler. A pinned version in this tag would be a second number to
# drift, which is how the gate came to run 1.93 while this built 1.88.
FROM rust:1-bookworm AS builder
WORKDIR /build

# Before any compile, so the toolchain install is its own cached layer and is not
# redone on a source change. `rustup show` is what triggers the install.
COPY rust-toolchain.toml ./
RUN rustup show

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

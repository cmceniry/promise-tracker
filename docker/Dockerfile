# Stage 1: Build frontend and API
FROM rust:1.92-bookworm AS builder

# Install system dependencies for building
RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# Add wasm target and install trunk (pre-built binary)
RUN rustup target add wasm32-unknown-unknown
RUN cargo install trunk

WORKDIR /build

# Copy the full workspace (needed for path dependencies between crates)
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY api/ api/
COPY cli/ cli/
COPY wpt/ wpt/
COPY frontend/ frontend/

# Build frontend WASM bundle
WORKDIR /build/frontend
RUN trunk build --release

# Build API server (embeds frontend/dist/ at compile time via rust-embed)
WORKDIR /build
RUN cargo build --release -p api

# Stage 2: Minimal runtime image
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/api /usr/local/bin/promise-tracker-api

# Default directory for contract storage
RUN mkdir -p /data/contracts
COPY samples/ /data/samples/

EXPOSE 8080

ENTRYPOINT ["promise-tracker-api"]
# 0.0.0.0 is required inside the container: isolation comes from the port publish
CMD ["--base-dir", "/data/contracts", "--port", "8080", "--host", "0.0.0.0"]

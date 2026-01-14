mod rust "promise_tracker/"

# Build frontend and backend into single binary
# build: build-frontend build-backend

# Build React frontend for production
build-frontend:
    npm run build

# # Build Rust backend with embedded frontend
# build-backend: build-frontend
#     cd promise_tracker/api && cargo build --release

# Development: Run API in dev mode (proxies to React dev server)
dev-api:
    cd promise_tracker/api && cargo run -- --dev

# Clean build artifacts
clean:
    rm -rf build
    cd promise_tracker && cargo clean

# Legacy targets (keep for compatibility)
start-clean:
    just rust build-wasm
    just rust sync-wptpkg
    npm start

run-server:
    npm start




[working-directory: './api']
build-backend:
    cargo build --release

[working-directory: './api']
run-dev:
    cargo run -- --dev

[working-directory: './wpt']
build-wasm:
    cargo build --release --target wasm32-unknown-unknown
    wasm-pack build --target web --weak-refs

[working-directory: './wpt']
sync-wptpkg:
    rsync -aHSPv --exclude .gitignore --delete ./pkg/ ../../src/wptpkg/

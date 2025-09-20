
[working-directory: './wpt']
build-wasm:
    cargo build --release --target wasm32-unknown-unknown
    wasm-pack build --target web --weak-refs

[working-directory: './wpt']
sync-wptpkg:
    rsync -aHSPv --exclude .gitignore --delete ./pkg/ ../../src/wptpkg/

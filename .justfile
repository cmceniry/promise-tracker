mod rust "promise_tracker/"

start-clean:
    just rust build-wasm
    just rust sync-wptpkg
    npm start

run-server:
    npm start

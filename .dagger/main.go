// A generated module for PromiseTracker functions
//
// This module has been generated via dagger init and serves as a reference to
// basic module structure as you get started with Dagger.
//
// Two functions have been pre-created. You can modify, delete, or add to them,
// as needed. They demonstrate usage of arguments and return types using simple
// echo and grep commands. The functions can be called from the dagger CLI or
// from one of the SDKs.
//
// The first line in this comment block is a short description line and the
// rest is a long description with more detail on the module's purpose or usage,
// if appropriate. All modules should have a short description.

package main

import (
	"context"
)

type PromiseTracker struct{}

func (m *PromiseTracker) rustBase() *Container {
	return dag.Container().From("rust:1.81.0").
		WithMountedCache("/.cargo", dag.CacheVolume("cargo")).
		WithExec([]string{"rustup", "target", "add", "wasm32-unknown-unknown"}).
		WithExec([]string{"cargo", "install", "wasm-pack"}).
		WithExec([]string{"cargo", "install", "wasm-opt"})
}

func (m *PromiseTracker) rust(
	src *Directory,
) *Container {
	return m.rustBase().
		WithMountedDirectory("/src", src).
		WithWorkdir("/src/wpt")
}

func (m *PromiseTracker) jsBase() *Container {
	return dag.Container().From("node:22").
		WithMountedCache("/.npm", dag.CacheVolume("npm")).
		WithExec([]string{"apt", "update"}).
		WithExec([]string{"apt", "install", "-y", "chromium"})
}

func (m *PromiseTracker) js(src *Directory) *Container {
	return m.jsBase().
		WithMountedDirectory("/src", src).
		WithWorkdir("/src").
		WithEntrypoint([]string{}).
		WithExec([]string{"npm", "install"})
}

func (m *PromiseTracker) PrepareContainers(
	src *Directory,
	wasmSrc *Directory,
) string {
	_ = m.rust(wasmSrc)
	_ = m.js(src)
	return "Success"
}

func (m *PromiseTracker) BuildWasm(
	ctx context.Context,
	src *Directory,
) *Directory {
	return m.rust(src).
		WithExec([]string{"cargo", "build", "--release", "--target", "wasm32-unknown-unknown"}).
		WithExec([]string{"wasm-pack", "build", "--target", "web", "--weak-refs"}).
		Directory("/src/wpt/pkg")
}

func (m *PromiseTracker) RunServer(
	ctx context.Context,
	src *Directory,
	wasmSrc *Directory,
) *Service {
	wpt := m.BuildWasm(ctx, wasmSrc)

	return m.js(src).
		WithMountedDirectory("/src/src/wptpkg", wpt).
		WithExec([]string{"npm", "run", "start"}).
		WithExposedPort(3000).
		AsService()
}

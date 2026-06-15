# AGENTS.md — proxima

Guidance for AI agents and contributors working in this repository.

## What this is

`proxima` is an async **forward proxy** with a rotating pool of upstream
(residential) proxies, plus an HTTP **control plane** for runtime management.

- Forward proxy is built on [`hudsucker`](https://crates.io/crates/hudsucker)
  (MITM HTTP/S) and listens on `127.0.0.1:8080`.
- Control plane is built on [`axum`](https://crates.io/crates/axum) and listens
  on `127.0.0.1:8000`.
- Both run concurrently from `main.rs` via `tokio::try_join!`.

## Layout

```
src/
  lib.rs           # Library crate: module exports, Result alias, router()
  main.rs          # Binary: wires axum control plane + hudsucker proxy
  ca.rs            # Persistent CA — load_or_create() from disk or generate
  proxy_pool.rs    # ProxyPool / ProxyEntry / SharedPool (Arc<RwLock<ProxyPool>>)
  proxy_handler.rs # RotatingProxyHandler — picks an upstream per request
  fetcher.rs       # fetch_proxy_list() — parse newline-delimited proxy lists
  routes.rs        # axum handlers: health_check, pool_stats, reload_pool
tests/
  common/mod.rs    # Shared test helpers (pool builders, body_to_string)
```

The crate is **both a library and a binary**. Library code lives in `lib.rs`
and is re-used by `main.rs` and by integration tests under `tests/`. Always add
new shared logic to a module exported from `lib.rs` so it stays testable.

## Build / Run / Test

```bash
cargo check                 # fast compile check — run before tests
cargo build --release       # optimized build
cargo run                   # start proxy (8080) + control plane (8000)

cargo test                  # run all unit + integration tests
cargo test -- --nocapture   # show println!/tracing output during tests
cargo test <name>           # run a single test by substring

cargo clippy -- -D warnings # lint; must pass clean
cargo fmt                   # format
```

## Control plane API

- `GET  /`               → health check (200 `OK`)
- `GET  /stats`          → `{"count": <pool_size>}`
- `POST /reload?url=...` → fetch + hot-swap the proxy list

## Testing conventions

- TDD: write a failing test, implement the minimal code to pass, then refactor.
  Run `cargo test` after every step.
- Unit tests live in a `#[cfg(test)] mod tests` block at the bottom of the
  module they exercise; they may touch private items.
- Integration tests live in `tests/` and use the public API via the `proxima::`
  crate path (e.g. `proxima::router`, `proxima::proxy_pool::ProxyPool`).
- HTTP is mocked with `mockito`; filesystem with `tempfile`; axum handlers are
  exercised with `tower`'s `ServiceExt::oneshot` (no real port needed).
- `hudsucker::HttpContext` is `#[non_exhaustive]` and **cannot** be constructed
  in tests. Test the proxy handler through the inherent `route_request` /
  `process_response` methods rather than the `HttpHandler` trait methods.

## Coding conventions (Rust)

- Prefer iterator chains (`.map()`/`.filter()`/`.fold()`) over manual loops.
- Model variants/actions/state as enums; pattern-match exhaustively.
- Use newtypes for domain concepts; make illegal states unrepresentable.
- Errors: `thiserror` enums for typed/recoverable errors; `color_eyre::Result`
  with `.context()` at the application layer. No `unwrap()`/`expect()` in
  production code paths (tests may use them).
- Borrow (`&T`) when reading, own when consuming, `&mut` only when mutating.
- Keep the public API at the crate root tidy; use `pub(crate)` for internals.

## Gotchas

- The `web-server` worktree path is an alias for this directory
  (`/Users/geramisadeghi/playgrounds/proxima`); prefer the real path.
- `ca/ca.key` is the TLS root of trust — never commit it (see `.gitignore`).
- The handler currently tags requests with an `UpstreamProxy` extension but the
  default connector does not yet route through it; see `PLAN.md` roadmap.

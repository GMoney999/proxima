# PLAN.md — ResProxy TDD Implementation Plan

> This plan is structured for a Warp terminal agent executing red-green TDD cycles.
> Each task follows the pattern: **write a failing test → make it pass → refactor**.
> Run `cargo test` after every step. No implementation code is written before its test.

---

## Ground Rules for the Agent

- Run `cargo test -- --nocapture` to see `println!` / `tracing` output during tests
- Run `cargo check` before `cargo test` to catch compile errors early
- One test at a time — do not write multiple failing tests simultaneously
- Commit after every green cycle: `git commit -m "green: <test name>"`
- If a test has been red for more than 2 fix attempts, check the design — don't hack it green

---

## Module Execution Order

```
1. proxy_pool
2. fetcher
3. ca
4. proxy_handler
5. routes
6. integration (full stack)
```

---

## 1. `proxy_pool` — Rotating Proxy Pool

### 1.1 — Empty pool returns `None` on pick

**Red:**
```rust
#[test]
fn empty_pool_pick_returns_none() {
    let pool = ProxyPool::default();
    assert!(pool.pick().is_none());
}
```
**Green:** Implement `ProxyPool::default()` with an empty `Vec`. `pick()` returns `None` when empty.

---

### 1.2 — Single entry pool always returns that entry

**Red:**
```rust
#[test]
fn single_entry_pool_always_returns_it() {
    let pool = ProxyPool::new(vec![ProxyEntry { uri: "http://1.2.3.4:8080".into() }]);
    for _ in 0..20 {
        assert_eq!(pool.pick().unwrap().uri, "http://1.2.3.4:8080");
    }
}
```
**Green:** Implement `ProxyPool::new(entries: Vec<ProxyEntry>)`. `pick()` returns `Some(&ProxyEntry)`.

---

### 1.3 — Multi-entry pool picks from all entries (distribution check)

**Red:**
```rust
#[test]
fn multi_entry_pool_picks_from_all_entries() {
    let pool = ProxyPool::new(vec![
        ProxyEntry { uri: "http://1.1.1.1:80".into() },
        ProxyEntry { uri: "http://2.2.2.2:80".into() },
        ProxyEntry { uri: "http://3.3.3.3:80".into() },
    ]);
    let picks: std::collections::HashSet<String> = (0..200)
        .map(|_| pool.pick().unwrap().uri.clone())
        .collect();
    assert_eq!(picks.len(), 3, "All entries should be reachable");
}
```
**Green:** Implement random selection via `rand::seq::IndexedRandom::choose`.

---

### 1.4 — `replace()` atomically swaps the pool

**Red:**
```rust
#[test]
fn replace_swaps_pool_entries() {
    let mut pool = ProxyPool::new(vec![ProxyEntry { uri: "http://old:80".into() }]);
    pool.replace(vec![ProxyEntry { uri: "http://new:80".into() }]);
    assert_eq!(pool.pick().unwrap().uri, "http://new:80");
    assert_eq!(pool.len(), 1);
}
```
**Green:** Implement `replace(&mut self, new_proxies: Vec<ProxyEntry>)`.

---

### 1.5 — `replace()` with empty list empties the pool

**Red:**
```rust
#[test]
fn replace_with_empty_empties_pool() {
    let mut pool = ProxyPool::new(vec![ProxyEntry { uri: "http://1.1.1.1:80".into() }]);
    pool.replace(vec![]);
    assert!(pool.pick().is_none());
    assert_eq!(pool.len(), 0);
}
```
**Green:** No extra logic needed if `replace` assigns directly — verify `pick()` handles empty `Vec`.

---

### 1.6 — `len()` reflects current pool size

**Red:**
```rust
#[test]
fn len_reflects_pool_size() {
    let pool = ProxyPool::new(vec![
        ProxyEntry { uri: "http://1.1.1.1:80".into() },
        ProxyEntry { uri: "http://2.2.2.2:80".into() },
    ]);
    assert_eq!(pool.len(), 2);
}
```
**Green:** Implement `len(&self) -> usize` returning `self.proxies.len()`.

---

### Edge Cases — `proxy_pool`

| Case | Test name | Expectation |
|---|---|---|
| Duplicate URIs in list | `duplicate_uris_are_preserved` | Both entries retained, both reachable |
| URI with auth credentials | `uri_with_credentials_preserved` | `http://user:pass@1.2.3.4:8080` stored verbatim |
| Very large pool (10k entries) | `large_pool_pick_does_not_panic` | `pick()` returns `Some` without panic |
| Pool replaced under concurrent reads | `concurrent_read_during_replace` | Use `Arc<RwLock<ProxyPool>>` — no deadlock or panic |

---

## 2. `fetcher` — Proxy List Fetcher

> Use `mockito` for HTTP mocking in these tests.

Add to `Cargo.toml`:
```toml
[dev-dependencies]
mockito = "1"
tokio = { version = "1", features = ["full"] }
```

---

### 2.1 — Parses a plain `ip:port` list

**Red:**
```rust
#[tokio::test]
async fn parses_plain_ip_port_list() {
    let mut server = mockito::Server::new_async().await;
    let mock = server.mock("GET", "/proxies.txt")
        .with_body("1.2.3.4:8080\n5.6.7.8:3128\n")
        .create_async().await;

    let entries = fetch_proxy_list(&format!("{}/proxies.txt", server.url())).await.unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].uri, "http://1.2.3.4:8080");
    assert_eq!(entries[1].uri, "http://5.6.7.8:3128");
    mock.assert_async().await;
}
```
**Green:** Implement `fetch_proxy_list(url: &str)`. Prepend `http://` when no scheme present.

---

### 2.2 — Ignores comment lines and blank lines

**Red:**
```rust
#[tokio::test]
async fn ignores_comments_and_blank_lines() {
    let mut server = mockito::Server::new_async().await;
    server.mock("GET", "/proxies.txt")
        .with_body("# comment\n\n1.2.3.4:8080\n  \n# another comment\n5.6.7.8:80\n")
        .create_async().await;

    let entries = fetch_proxy_list(&format!("{}/proxies.txt", server.url())).await.unwrap();
    assert_eq!(entries.len(), 2);
}
```
**Green:** Filter lines starting with `#` and blank/whitespace-only lines.

---

### 2.3 — Preserves explicit schemes (`socks5://`, `http://`)

**Red:**
```rust
#[tokio::test]
async fn preserves_explicit_schemes() {
    let mut server = mockito::Server::new_async().await;
    server.mock("GET", "/proxies.txt")
        .with_body("socks5://1.2.3.4:1080\nhttp://5.6.7.8:8080\n")
        .create_async().await;

    let entries = fetch_proxy_list(&format!("{}/proxies.txt", server.url())).await.unwrap();
    assert_eq!(entries[0].uri, "socks5://1.2.3.4:1080");
    assert_eq!(entries[1].uri, "http://5.6.7.8:8080");
}
```
**Green:** Only prepend `http://` when `!line.contains("://")`.

---

### 2.4 — Returns error on non-200 response

**Red:**
```rust
#[tokio::test]
async fn returns_error_on_non_200() {
    let mut server = mockito::Server::new_async().await;
    server.mock("GET", "/proxies.txt")
        .with_status(404)
        .create_async().await;

    let result = fetch_proxy_list(&format!("{}/proxies.txt", server.url())).await;
    assert!(result.is_err());
}
```
**Green:** Call `.error_for_status()?` on the `reqwest` response before reading body.

---

### 2.5 — Returns empty `Vec` on body with only comments

**Red:**
```rust
#[tokio::test]
async fn empty_vec_on_all_comments() {
    let mut server = mockito::Server::new_async().await;
    server.mock("GET", "/proxies.txt")
        .with_body("# only comments\n# nothing here\n")
        .create_async().await;

    let entries = fetch_proxy_list(&format!("{}/proxies.txt", server.url())).await.unwrap();
    assert!(entries.is_empty());
}
```
**Green:** Filtering logic from 2.2 already covers this — verify it passes.

---

### Edge Cases — `fetcher`

| Case | Test name | Expectation |
|---|---|---|
| Trailing whitespace on entries | `trims_whitespace_from_entries` | `"1.2.3.4:8080  "` → `"http://1.2.3.4:8080"` |
| Windows-style `\r\n` line endings | `handles_crlf_line_endings` | Parsed correctly, no `\r` in URI |
| Malformed entry (no port) | `malformed_entry_is_skipped` | `"notanip"` skipped or wrapped with warning |
| Empty response body | `empty_body_returns_empty_vec` | Returns `Ok(vec![])` |
| Network timeout | `network_timeout_returns_error` | Returns `Err(...)` — mock a slow server |
| Very large list (50k lines) | `large_list_parses_fully` | All valid entries parsed, no OOM |

---

## 3. `ca` — Persistent Certificate Authority

### 3.1 — Generates CA files when none exist

**Red:**
```rust
#[tokio::test]
async fn generates_ca_files_when_none_exist() {
    let dir = tempfile::tempdir().unwrap();
    load_or_create(dir.path()).await.unwrap();
    assert!(dir.path().join("ca.cer").exists());
    assert!(dir.path().join("ca.key").exists());
}
```
Add `tempfile` to dev-dependencies:
```toml
[dev-dependencies]
tempfile = "3"
```
**Green:** Implement `load_or_create`. On missing files, call `generate_ca_pem()` and write both.

---

### 3.2 — Reuses existing CA files on second call

**Red:**
```rust
#[tokio::test]
async fn reuses_existing_ca_on_second_call() {
    let dir = tempfile::tempdir().unwrap();
    load_or_create(dir.path()).await.unwrap();
    let cert_first = tokio::fs::read_to_string(dir.path().join("ca.cer")).await.unwrap();

    load_or_create(dir.path()).await.unwrap();
    let cert_second = tokio::fs::read_to_string(dir.path().join("ca.cer")).await.unwrap();

    assert_eq!(cert_first, cert_second, "CA cert should not be regenerated");
}
```
**Green:** Check `files.both_exist()` before generating — covered by `load_from_disk` branch.

---

### 3.3 — Returns error on corrupt key file

**Red:**
```rust
#[tokio::test]
async fn returns_error_on_corrupt_key_file() {
    let dir = tempfile::tempdir().unwrap();
    tokio::fs::write(dir.path().join("ca.cer"), "not a cert").await.unwrap();
    tokio::fs::write(dir.path().join("ca.key"), "not a key").await.unwrap();

    let result = load_or_create(dir.path()).await;
    assert!(result.is_err());
}
```
**Green:** `KeyPair::from_pem` and `Issuer::from_ca_cert_pem` propagate errors — ensure `.wrap_err()` is in place.

---

### 3.4 — Creates CA directory if it does not exist

**Red:**
```rust
#[tokio::test]
async fn creates_ca_directory_if_missing() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("nested").join("ca");
    load_or_create(&nested).await.unwrap();
    assert!(nested.join("ca.cer").exists());
}
```
**Green:** `fs::create_dir_all(ca_dir)` at the top of `load_or_create` covers this.

---

### Edge Cases — `ca`

| Case | Test name | Expectation |
|---|---|---|
| Only `ca.cer` exists, `ca.key` missing | `partial_ca_files_triggers_regeneration` | Treat as missing — regenerate both |
| Only `ca.key` exists, `ca.cer` missing | `partial_ca_files_triggers_regeneration` | Same as above |
| CA directory is a file, not a dir | `ca_path_is_file_returns_error` | Returns `Err(...)` cleanly |
| Read-only CA directory | `readonly_dir_returns_error` | Write fails with descriptive error |
| Generated cert is valid PEM | `generated_cert_is_valid_pem` | `cert_pem` contains `-----BEGIN CERTIFICATE-----` |
| Generated key is valid PEM | `generated_key_is_valid_pem` | `key_pem` contains `-----BEGIN PRIVATE KEY-----` |

---

## 4. `proxy_handler` — hudsucker HttpHandler

> These tests mock the shared pool and verify handler behavior without starting a real proxy.

### 4.1 — Passes request through when pool has entries

**Red:**
```rust
#[tokio::test]
async fn passes_request_through_with_upstream_extension() {
    let pool = Arc::new(RwLock::new(ProxyPool::new(vec![
        ProxyEntry { uri: "http://1.2.3.4:8080".into() },
    ])));
    let mut handler = RotatingProxyHandler { pool };
    let req = Request::builder().uri("http://example.com").body(Body::empty()).unwrap();
    let ctx = HttpContext { /* .. */ };

    let result = handler.handle_request(&ctx, req).await;
    let req = match result {
        RequestOrResponse::Request(r) => r,
        _ => panic!("Expected request to be passed through"),
    };
    assert!(req.extensions().get::<UpstreamProxy>().is_some());
}
```
**Green:** In `handle_request`, insert `UpstreamProxy` extension and return `req.into()`.

---

### 4.2 — Returns 503 when pool is empty

**Red:**
```rust
#[tokio::test]
async fn returns_503_when_pool_is_empty() {
    let pool = Arc::new(RwLock::new(ProxyPool::default()));
    let mut handler = RotatingProxyHandler { pool };
    let req = Request::builder().uri("http://example.com").body(Body::empty()).unwrap();
    let ctx = HttpContext { /* .. */ };

    let result = handler.handle_request(&ctx, req).await;
    let resp = match result {
        RequestOrResponse::Response(r) => r,
        _ => panic!("Expected 503 response"),
    };
    assert_eq!(resp.status(), 503);
}
```
**Green:** Return `Response::builder().status(503).body(Body::empty()).unwrap().into()` when `pool.pick()` is `None`.

---

### 4.3 — Response handler passes through unchanged

**Red:**
```rust
#[tokio::test]
async fn response_handler_passes_through_unchanged() {
    let pool = Arc::new(RwLock::new(ProxyPool::default()));
    let mut handler = RotatingProxyHandler { pool };
    let resp = Response::builder().status(200).body(Body::empty()).unwrap();
    let ctx = HttpContext { /* .. */ };

    let result = handler.handle_response(&ctx, resp).await;
    assert_eq!(result.status(), 200);
}
```
**Green:** `handle_response` returns `res` unchanged.

---

### Edge Cases — `proxy_handler`

| Case | Test name | Expectation |
|---|---|---|
| Pool emptied between request cycles | `pool_empty_mid_flight_returns_503` | `pick()` returns `None` → 503 |
| Handler is cloned across tasks | `cloned_handler_shares_pool` | Both clones see the same pool state |
| Extension already set on request | `upstream_extension_is_overwritten` | New `UpstreamProxy` replaces old one |
| Concurrent requests pick different upstreams | `concurrent_requests_rotate_upstreams` | With 2-entry pool, both URIs appear across 100 concurrent requests |

---

## 5. `routes` — axum Control Plane

> Use `axum::test` helpers for handler tests without binding a port.

### 5.1 — `GET /` returns 200

**Red:**
```rust
#[tokio::test]
async fn health_check_returns_200() {
    let app = router_with_pool(empty_pool());
    let resp = app.oneshot(Request::builder().uri("/").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
```
**Green:** `health_check` handler returns `StatusCode::OK`.

---

### 5.2 — `GET /stats` returns pool count as JSON

**Red:**
```rust
#[tokio::test]
async fn stats_returns_pool_count() {
    let pool = pool_with_entries(3);
    let app = router_with_pool(pool);
    let resp = app.oneshot(Request::builder().uri("/stats").body(Body::empty()).unwrap()).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_to_string(resp).await;
    assert_eq!(body, r#"{"count":3}"#);
}
```
**Green:** `pool_stats` handler reads `pool.read().unwrap().len()` and returns `Json(PoolStats { count })`.

---

### 5.3 — `POST /reload` with valid URL updates pool

**Red:**
```rust
#[tokio::test]
async fn reload_updates_pool_from_url() {
    let mut server = mockito::Server::new_async().await;
    server.mock("GET", "/list.txt").with_body("1.1.1.1:80\n2.2.2.2:80\n").create_async().await;

    let pool = empty_pool();
    let app = router_with_pool(pool.clone());
    let url = format!("{}/list.txt", server.url());

    let resp = app.oneshot(
        Request::builder()
            .method("POST")
            .uri(format!("/reload?url={}", url))
            .body(Body::empty())
            .unwrap()
    ).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(pool.read().unwrap().len(), 2);
}
```
**Green:** `reload_pool` fetches list, calls `pool.write().unwrap().replace(entries)`.

---

### 5.4 — `POST /reload` without `url` param returns 400

**Red:**
```rust
#[tokio::test]
async fn reload_without_url_returns_400() {
    let app = router_with_pool(empty_pool());
    let resp = app.oneshot(
        Request::builder().method("POST").uri("/reload").body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
```
**Green:** Guard `params.get("url")` with early return of `400`.

---

### 5.5 — `POST /reload` with unreachable URL returns 500

**Red:**
```rust
#[tokio::test]
async fn reload_with_bad_url_returns_500() {
    let app = router_with_pool(empty_pool());
    let resp = app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/reload?url=http://localhost:1")
            .body(Body::empty())
            .unwrap()
    ).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
```
**Green:** `fetch_proxy_list` returns `Err` → route returns `500`.

---

### Edge Cases — `routes`

| Case | Test name | Expectation |
|---|---|---|
| `/reload` URL returns empty list | `reload_with_empty_list_clears_pool` | Pool becomes empty, returns 200 |
| `/reload` called twice in quick succession | `concurrent_reloads_do_not_corrupt_pool` | Pool reflects last write, no panic |
| `/stats` with 0 entries | `stats_returns_zero_when_pool_empty` | `{"count":0}` |
| Unknown route | `unknown_route_returns_404` | `StatusCode::NOT_FOUND` |

---

## 6. Integration Tests

> Spin up the full stack against real bound ports using `tokio::time::timeout`.

```toml
[dev-dependencies]
reqwest = { version = "0.12", features = ["socks"] }
tempfile = "3"
mockito = "1"
```

---

### 6.1 — Proxy forwards HTTP request through upstream

**Red:**
```rust
#[tokio::test]
async fn proxy_forwards_http_request() {
    // 1. Start a mock target server
    // 2. Start a mock upstream proxy
    // 3. Load upstream into pool
    // 4. Send request through hudsucker on a random port
    // 5. Assert target server received the request
}
```

---

### 6.2 — Proxy returns 503 when pool is empty on startup

**Red:**
```rust
#[tokio::test]
async fn proxy_returns_503_with_empty_pool() {
    // Start proxy with empty pool
    // Send any HTTP request
    // Assert 503 response
}
```

---

### 6.3 — Hot reload reflects in subsequent requests

**Red:**
```rust
#[tokio::test]
async fn hot_reload_changes_upstream_mid_flight() {
    // 1. Start proxy with pool containing upstream A
    // 2. Reload pool via POST /reload with upstream B
    // 3. Assert subsequent requests route through B
}
```

---

### 6.4 — Control plane stays responsive under proxy load

**Red:**
```rust
#[tokio::test]
async fn control_plane_responds_under_load() {
    // Spawn 50 concurrent proxy requests
    // Simultaneously poll GET /stats 10 times
    // Assert all /stats calls return 200 within 500ms
}
```

---

### Edge Cases — Integration

| Case | Test name | Expectation |
|---|---|---|
| Upstream proxy goes offline mid-pool | `dead_upstream_causes_502_or_retry` | Returns error or retries with another upstream |
| HTTPS `CONNECT` tunnel established | `https_connect_tunnel_succeeds` | TLS handshake succeeds with trusted CA |
| Proxy handles 100 concurrent connections | `high_concurrency_no_panic` | No panics, no deadlocks |
| Graceful shutdown drains in-flight requests | `graceful_shutdown_completes_inflight` | In-flight requests complete before process exits |
| Reload during active connections | `reload_during_active_connections` | Existing connections unaffected, new ones use new pool |

---

## Test Utilities (shared `tests/common/mod.rs`)

```rust title="tests/common/mod.rs"
use std::sync::{Arc, RwLock};
use resproxy::proxy_pool::{ProxyEntry, ProxyPool, SharedPool};
use axum::{Router, body::Body};

pub fn empty_pool() -> SharedPool {
    Arc::new(RwLock::new(ProxyPool::default()))
}

pub fn pool_with_entries(n: usize) -> SharedPool {
    let entries = (0..n)
        .map(|i| ProxyEntry { uri: format!("http://1.1.1.{}:80", i) })
        .collect();
    Arc::new(RwLock::new(ProxyPool::new(entries)))
}

pub fn router_with_pool(pool: SharedPool) -> axum::Router {
    resproxy::router(pool)
}

pub async fn body_to_string(resp: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    String::from_utf8(bytes).unwrap()
}
```

---

## Completion Checklist

```
[ ] proxy_pool  — 6 tests green + edge cases
[ ] fetcher     — 5 tests green + edge cases
[ ] ca          — 4 tests green + edge cases
[ ] proxy_handler — 3 tests green + edge cases
[ ] routes      — 5 tests green + edge cases
[ ] integration — 4 tests green + edge cases
[ ] cargo clippy -- -D warnings  passes clean
[ ] cargo test --release          all green
[ ] README client trust steps verified manually
```

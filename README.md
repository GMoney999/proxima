# 🔄 ResProxy — Residential Rotating IP Pool Forward Proxy

A high-performance, async forward proxy written in Rust that routes outbound traffic through a rotating pool of residential proxies sourced from public lists. Built on [`hudsucker`](https://crates.io/crates/hudsucker) and [`axum`](https://crates.io/crates/axum).

---

## Features

- 🔄 **Rotating upstream proxies** — random selection from a live pool on every request
- 📋 **Public list ingestion** — load proxies from any newline-delimited HTTP/HTTPS URL
- 🔁 **Hot reload** — swap the proxy list at runtime without restarting
- 🔐 **Persistent CA** — self-signed CA generated once, saved to disk, survives restarts
- 📊 **Control plane API** — `axum`-powered HTTP API for health checks, stats, and reloads
- ⚡ **Fully async** — built on `tokio` with zero blocking I/O
- 🛡️ **HTTPS tunneling** — full TLS support via `rustls` and dynamic certificate generation

---

## Architecture

```
Client
  │
  │  HTTP / HTTPS (CONNECT)
  ▼
hudsucker Forward Proxy  (port 8080)
  │
  │  Reads from shared Arc<RwLock<ProxyPool>>
  │  Picks a random upstream on every request
  ▼
Upstream Residential Proxy  (ip:port from public list)
  │
  ▼
Internet
  
─────────────────────────────────────────

axum Control Plane  (port 8000)
  ├── GET  /          → health check
  ├── GET  /stats     → pool size & status
  └── POST /reload    → hot-swap proxy list from URL
```

---

## Project Structure

```
resproxy/
├── src/
│   ├── main.rs           # Entry point — wires axum + hudsucker together
│   ├── ca.rs             # Persistent CA — load from disk or generate fresh
│   ├── proxy_pool.rs     # Shared rotating proxy pool (Arc<RwLock<ProxyPool>>)
│   ├── proxy_handler.rs  # hudsucker HttpHandler — upstream selection logic
│   ├── fetcher.rs        # Fetches & parses public proxy lists over HTTP
│   └── routes.rs         # axum route handlers (health, stats, reload)
├── ca/
│   ├── ca.cer            # CA certificate — install this in your clients
│   └── ca.key            # CA private key  — keep this secret, never commit
├── .gitignore
├── Cargo.toml
└── README.md
```

---

## Prerequisites

- [Rust](https://rustup.rs/) stable (1.75+)
- `cargo` in your `PATH`

---

## Getting Started

### 1. Clone & Build

```bash
git clone https://github.com/you/resproxy.git
cd resproxy
cargo build --release
```

### 2. Run

```bash
cargo run --release
```

On first run, a CA key pair is generated and saved to `./ca/`:

```
INFO  New CA saved to disk — install `ca/ca.cer` in your clients
INFO  Control plane listening on 127.0.0.1:8000
INFO  Forward proxy listening on 127.0.0.1:8080
```

### 3. Trust the CA Certificate

Clients must trust `ca/ca.cer` **once** to avoid TLS errors on HTTPS traffic.

| Platform       | Command                                                                                          |
|----------------|--------------------------------------------------------------------------------------------------|
| Linux (system) | `sudo cp ca/ca.cer /usr/local/share/ca-certificates/resproxy.crt && sudo update-ca-certificates` |
| macOS          | `sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain ca/ca.cer` |
| Firefox        | Settings → Privacy & Security → Certificates → View Certificates → Import                        |
| Chrome / Edge  | Settings → Privacy → Manage Certificates → Trusted Root CAs → Import                             |
| curl           | `curl --cacert ca/ca.cer https://example.com`                                                    |
| reqwest (Rust) | `ClientBuilder::new().add_root_certificate(Certificate::from_pem(&ca_bytes)?)`                   |

### 4. Load a Proxy List

```bash
curl -X POST "http://127.0.0.1:8000/reload?url=https://example.com/proxy-list.txt"
```

The list must be a plain-text file with one proxy per line:

```
# Lines starting with # are ignored
1.2.3.4:8080
5.6.7.8:3128
socks5://9.10.11.12:1080
http://13.14.15.16:8888
```

Entries without a scheme are assumed to be `http://`.

### 5. Route Traffic Through the Proxy

```bash
# curl
curl -x http://127.0.0.1:8080 https://ifconfig.me

# wget
wget -e use_proxy=yes -e http_proxy=http://127.0.0.1:8080 https://ifconfig.me

# Set system-wide (Linux/macOS)
export HTTP_PROXY=http://127.0.0.1:8080
export HTTPS_PROXY=http://127.0.0.1:8080
```

---

## Control Plane API

Base URL: `http://127.0.0.1:8000`

| Method | Endpoint          | Description                              | Example                                      |
|--------|-------------------|------------------------------------------|----------------------------------------------|
| `GET`  | `/`               | Health check                             | `curl http://127.0.0.1:8000/`                |
| `GET`  | `/stats`          | Returns current pool size as JSON        | `curl http://127.0.0.1:8000/stats`           |
| `POST` | `/reload?url=...` | Fetch a new proxy list and hot-swap pool | `curl -X POST "http://127.0.0.1:8000/reload?url=..."` |

### Example Responses

```jsonc
// GET /stats
{ "count": 312 }
```

---

## Configuration

Configuration is currently handled via constants and startup arguments. Environment variable support is planned.

| Setting          | Default           | Location              | Description                          |
|------------------|-------------------|-----------------------|--------------------------------------|
| Control plane    | `127.0.0.1:8000`  | `main.rs`             | Address for the axum API server      |
| Proxy port       | `127.0.0.1:8080`  | `main.rs`             | Address for the hudsucker proxy      |
| CA directory     | `./ca`            | `main.rs`             | Where `ca.cer` and `ca.key` are stored |
| Cert cache size  | `1000`            | `ca.rs`               | In-memory TLS cert cache (per host)  |
| CA validity      | 2024–2034         | `ca.rs`               | Lifetime of the generated CA cert    |

---

## Cargo.toml Dependencies

```toml
[dependencies]
axum           = "0.8"
tokio          = { version = "1", features = ["full"] }
hudsucker      = "0.24"
rcgen          = "0.13"
reqwest        = { version = "0.12", features = ["socks"] }
rand           = "0.9"
color-eyre     = "0.6"
tracing        = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-error  = "0.2"
```

---

## Security Considerations

> ⚠️ **This project is intended for research, testing, and controlled environments only.**

- **Never commit `ca/ca.key`** — it is the root of trust for all proxied TLS traffic. Add it to `.gitignore`.
- **Public proxy lists are untrusted** — proxies can log, modify, or drop your traffic. Do not route sensitive traffic through unverified upstreams.
- **No authentication** — the proxy is currently open. Restrict access via firewall rules (`iptables`, security groups) or add `Proxy-Authorization` header validation in `proxy_handler.rs`.
- **MITM by design** — the CA intercepts TLS to enable HTTPS proxying. Clients that trust `ca.cer` are subject to traffic inspection by this proxy.

---

## Roadmap

- [ ] Proxy health checking — background task to remove dead upstreams
- [ ] Weighted / round-robin rotation strategies
- [ ] Per-upstream request rate limiting
- [ ] `Proxy-Authorization` authentication on the proxy port
- [ ] Environment variable / TOML config file support
- [ ] Retry on upstream failure with a different proxy
- [ ] Prometheus metrics endpoint (`/metrics`)
- [ ] Docker image & `docker-compose.yml`
- [ ] Persistent proxy pool (save/restore from disk across restarts)

---

## License

MIT © Gerami

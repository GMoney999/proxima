//! Integration test that boots the real hudsucker forward proxy and verifies
//! the empty-pool rejection path end-to-end.
//!
//! Note: the "forwards through an upstream proxy" scenarios from PLAN.md (§6.1,
//! §6.3 routing) are intentionally not implemented here. The handler tags
//! requests with an `UpstreamProxy` extension, but the default rustls connector
//! does not yet route through it (see the roadmap in `README.md`/`AGENTS.md`),
//! so there is no upstream routing to assert. The upstream-selection logic
//! itself is covered by the `proxy_handler` unit tests.

mod common;

use common::{empty_pool, free_port};
use proxima::hudsucker::{Proxy, rustls::crypto::aws_lc_rs};
use proxima::proxy_handler::RotatingProxyHandler;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

#[tokio::test]
async fn proxy_returns_503_with_empty_pool() {
    let ca_dir = tempfile::tempdir().unwrap();
    let ca = proxima::ca::load_or_create(ca_dir.path()).await.unwrap();

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, free_port()));
    let proxy = Proxy::builder()
        .with_addr(addr)
        .with_ca(ca)
        .with_rustls_connector(aws_lc_rs::default_provider())
        .with_http_handler(RotatingProxyHandler { pool: empty_pool() })
        .build()
        .expect("failed to build proxy");

    tokio::spawn(async move {
        proxy.start().await.ok();
    });

    // Give the listener a moment to bind.
    tokio::time::sleep(Duration::from_millis(150)).await;

    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::http(format!("http://{addr}")).unwrap())
        .build()
        .unwrap();

    // Plain HTTP request is intercepted by the proxy; with an empty pool the
    // handler short-circuits with 503 before any upstream connection.
    let resp = tokio::time::timeout(
        Duration::from_secs(10),
        client.get("http://example.com/").send(),
    )
    .await
    .expect("request timed out")
    .expect("request failed");

    assert_eq!(resp.status(), 503);
}

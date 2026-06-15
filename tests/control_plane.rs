//! End-to-end tests for the axum control plane, served over a real loopback
//! TCP port and driven with a `reqwest` client.

mod common;

use common::{empty_pool, pool_with_entries};
use proxima::proxy_pool::SharedPool;
use std::net::{Ipv4Addr, SocketAddr};

/// Bind the control-plane router to an ephemeral port and serve it in the
/// background, returning the bound address.
async fn serve(pool: SharedPool) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let app = proxima::router(pool);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[tokio::test]
async fn health_check_responds_ok() {
    let addr = serve(empty_pool()).await;
    let resp = reqwest::get(format!("http://{addr}/")).await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "OK");
}

#[tokio::test]
async fn stats_reports_pool_count() {
    let addr = serve(pool_with_entries(3)).await;
    let body = reqwest::get(format!("http://{addr}/stats"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(body, r#"{"count":3}"#);
}

#[tokio::test]
async fn hot_reload_reflects_in_subsequent_stats() {
    let mut upstream = mockito::Server::new_async().await;
    upstream
        .mock("GET", "/list.txt")
        .with_body("alpha:80\nbeta:80\n")
        .create_async()
        .await;

    let pool = empty_pool();
    let addr = serve(pool.clone()).await;
    let client = reqwest::Client::new();

    // Initially empty.
    let before = client
        .get(format!("http://{addr}/stats"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(before, r#"{"count":0}"#);

    // Hot-swap the pool from the mocked list.
    let list_url = format!("{}/list.txt", upstream.url());
    let reload = client
        .post(format!("http://{addr}/reload?url={list_url}"))
        .send()
        .await
        .unwrap();
    assert_eq!(reload.status(), 200);

    // Subsequent stats reflect the new pool.
    let after = client
        .get(format!("http://{addr}/stats"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(after, r#"{"count":2}"#);
}

#[tokio::test]
async fn reload_without_url_returns_400() {
    let addr = serve(empty_pool()).await;
    let resp = reqwest::Client::new()
        .post(format!("http://{addr}/reload"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

use crate::{fetcher::fetch_proxy_list, proxy_pool::SharedPool};
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Serialize;

pub async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

#[derive(Serialize)]
pub struct PoolStats {
    pub count: usize,
}

pub async fn pool_stats(State(pool): State<SharedPool>) -> impl IntoResponse {
    let count = pool.read().unwrap().len();
    Json(PoolStats { count })
}

/// POST /reload?url=http://... — re-fetches and hot-swaps the proxy list
pub async fn reload_pool(
    State(pool): State<SharedPool>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(url) = params.get("url") else {
        return (StatusCode::BAD_REQUEST, "Missing `url` query param").into_response();
    };

    match fetch_proxy_list(url).await {
        Ok(entries) => {
            let count = entries.len();
            pool.write().unwrap().replace(entries);
            (StatusCode::OK, format!("Loaded {count} proxies")).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to fetch proxy list: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch list").into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        proxy_pool::{ProxyEntry, ProxyPool},
        router,
    };
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        response::Response,
    };
    use std::sync::{Arc, RwLock};
    use tower::util::ServiceExt;

    fn empty_pool() -> crate::proxy_pool::SharedPool {
        Arc::new(RwLock::new(ProxyPool::default()))
    }

    fn pool_with_entries(n: usize) -> crate::proxy_pool::SharedPool {
        let entries = (0..n)
            .map(|i| ProxyEntry {
                uri: format!("http://proxy-{i}:80"),
            })
            .collect();
        Arc::new(RwLock::new(ProxyPool::new(entries)))
    }

    async fn body_to_string(resp: Response) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    fn get(uri: &str) -> Request<Body> {
        Request::builder().uri(uri).body(Body::empty()).unwrap()
    }

    fn post(uri: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn health_check_returns_200() {
        let resp = router(empty_pool()).oneshot(get("/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn stats_returns_pool_count() {
        let resp = router(pool_with_entries(3))
            .oneshot(get("/stats"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(body_to_string(resp).await, r#"{"count":3}"#);
    }

    #[tokio::test]
    async fn reload_updates_pool_from_url() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/list.txt")
            .with_body("alpha:80\nbeta:80\n")
            .create_async()
            .await;

        let pool = empty_pool();
        let url = format!("{}/list.txt", server.url());
        let resp = router(pool.clone())
            .oneshot(post(&format!("/reload?url={url}")))
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(pool.read().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn reload_without_url_returns_400() {
        let resp = router(empty_pool()).oneshot(post("/reload")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn reload_with_bad_url_returns_500() {
        let resp = router(empty_pool())
            .oneshot(post("/reload?url=http://localhost:1"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ── Edge cases ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn stats_returns_zero_when_pool_empty() {
        let resp = router(empty_pool()).oneshot(get("/stats")).await.unwrap();
        assert_eq!(body_to_string(resp).await, r#"{"count":0}"#);
    }

    #[tokio::test]
    async fn reload_with_empty_list_clears_pool() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/empty.txt")
            .with_body("# nothing\n")
            .create_async()
            .await;

        let pool = pool_with_entries(5);
        let url = format!("{}/empty.txt", server.url());
        let resp = router(pool.clone())
            .oneshot(post(&format!("/reload?url={url}")))
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(pool.read().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn unknown_route_returns_404() {
        let resp = router(empty_pool())
            .oneshot(get("/does-not-exist"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}

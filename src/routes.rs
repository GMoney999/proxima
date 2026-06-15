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

//! proxima — async forward proxy with a rotating upstream pool and an axum
//! control plane.
//!
//! This library crate exposes the building blocks used by both the `proxima`
//! binary (`main.rs`) and the integration tests under `tests/`.

pub mod ca;
pub mod fetcher;
pub mod proxy_handler;
pub mod proxy_pool;
pub mod routes;

// Re-exported so integration tests (and downstream users) can build the proxy
// stack without depending on a specific hudsucker version directly.
pub use hudsucker;

use axum::{
    Router,
    routing::{get, post},
};
use proxy_pool::SharedPool;

/// Application-layer result type with rich error context.
pub type Result<T> = color_eyre::eyre::Result<T>;

/// Builds the axum control-plane router wired to the shared proxy pool.
///
/// Routes:
/// - `GET  /`        → health check
/// - `GET  /stats`   → pool size as JSON
/// - `POST /reload`  → hot-swap the pool from a URL
pub fn router(pool: SharedPool) -> Router {
    Router::new()
        .route("/", get(routes::health_check))
        .route("/stats", get(routes::pool_stats))
        .route("/reload", post(routes::reload_pool))
        .with_state(pool)
}

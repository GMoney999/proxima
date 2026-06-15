#![allow(dead_code, unused)]

mod ca;
mod fetcher;
mod proxy_handler;
mod proxy_pool;
mod routes;

use axum::{
    Router,
    routing::{get, post},
};
use hudsucker::{
    Proxy,
    certificate_authority::RcgenAuthority,
    rcgen::{self, CertificateParams, DistinguishedName, Issuer, KeyPair},
    rustls::crypto::aws_lc_rs,
};
use proxy_handler::RotatingProxyHandler;
use proxy_pool::{ProxyPool, SharedPool};
use std::{
    net::SocketAddr,
    sync::{Arc, RwLock},
};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub type Result<T> = color_eyre::eyre::Result<T>;

/// Generates a transient self-signed CA entirely in memory.
/// No files, no setup — regenerated fresh on every startup.
fn build_ca() -> RcgenAuthority {
    let key_pair = KeyPair::generate().expect("Failed to generate CA key pair");

    let mut params = CertificateParams::default();
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.distinguished_name = {
        let mut dn = DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, "Rotating Proxy CA");
        dn
    };

    let issuer = Issuer::new(params, key_pair);

    RcgenAuthority::new(issuer, 1_000, aws_lc_rs::default_provider())
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().pretty())
        .with(EnvFilter::from_default_env())
        .init();

    // Shared proxy pool — start empty, populate via /reload
    let pool: SharedPool = Arc::new(RwLock::new(ProxyPool::default()));

    // ── Axum control plane (port 8000) ──────────────────────────────────────
    let control_router = Router::new()
        .route("/", get(routes::health_check))
        .route("/stats", get(routes::pool_stats))
        .route("/reload", post(routes::reload_pool))
        .with_state(pool.clone());

    let control_addr = "127.0.0.1:8000";
    let listener = tokio::net::TcpListener::bind(control_addr).await?;
    tracing::info!("Control plane listening on {control_addr}");

    // ── hudsucker forward proxy (port 8080) ──────────────────────────────────
    // NoopAuthority: tunnels CONNECT without MITM — no CA cert needed
    let proxy = Proxy::builder()
        .with_addr(SocketAddr::from(([127, 0, 0, 1], 8080)))
        .with_ca(build_ca()) // ← replaces NoopAuthority
        .with_rustls_connector(aws_lc_rs::default_provider())
        .with_http_handler(RotatingProxyHandler { pool: pool.clone() })
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .build()
        .expect("Failed to build proxy");

    // Run both concurrently
    tokio::try_join!(
        async {
            axum::serve(listener, control_router)
                .await
                .map_err(|e| color_eyre::eyre::eyre!(e))
        },
        async { proxy.start().await.map_err(|e| color_eyre::eyre::eyre!(e)) },
    )?;

    Ok(())
}

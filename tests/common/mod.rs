#![allow(dead_code)]

use proxima::proxy_pool::{ProxyEntry, ProxyPool, SharedPool};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, RwLock};

pub fn empty_pool() -> SharedPool {
    Arc::new(RwLock::new(ProxyPool::default()))
}

pub fn pool_with_entries(n: usize) -> SharedPool {
    let entries = (0..n)
        .map(|i| ProxyEntry {
            uri: format!("http://proxy-{i}:80"),
        })
        .collect();
    Arc::new(RwLock::new(ProxyPool::new(entries)))
}

pub fn router_with_pool(pool: SharedPool) -> axum::Router {
    proxima::router(pool)
}

/// Grab an ephemeral free TCP port on loopback. There is a small race between
/// closing the probe socket and re-binding, which is acceptable for tests.
pub fn free_port() -> u16 {
    std::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

use rand::seq::IndexedRandom;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone)]
pub struct ProxyEntry {
    /// Full URI, e.g. "http://1.2.3.4:8080" or "socks5://1.2.3.4:1080"
    pub uri: String,
}

#[derive(Debug, Default)]
pub struct ProxyPool {
    proxies: Vec<ProxyEntry>,
}

impl ProxyPool {
    pub fn new(proxies: Vec<ProxyEntry>) -> Self {
        Self { proxies }
    }

    /// Replace the pool's entries atomically (called on reload).
    pub fn replace(&mut self, new_proxies: Vec<ProxyEntry>) {
        self.proxies = new_proxies;
        tracing::info!("Proxy pool updated: {} entries", self.proxies.len());
    }

    /// Pick a random upstream proxy URI.
    pub fn pick(&self) -> Option<&ProxyEntry> {
        let mut rng = rand::rng();
        self.proxies.choose(&mut rng)
    }

    pub fn len(&self) -> usize {
        self.proxies.len()
    }
}

pub type SharedPool = Arc<RwLock<ProxyPool>>;

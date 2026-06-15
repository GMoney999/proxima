use rand::seq::IndexedRandom;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone)]
pub struct ProxyEntry {
    /// Full URI, e.g. "http://proxy-a:8080" or "socks5://proxy-b:1080"
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

    pub fn is_empty(&self) -> bool {
        self.proxies.is_empty()
    }
}

pub type SharedPool = Arc<RwLock<ProxyPool>>;

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(uri: &str) -> ProxyEntry {
        ProxyEntry { uri: uri.into() }
    }

    #[test]
    fn empty_pool_pick_returns_none() {
        let pool = ProxyPool::default();
        assert!(pool.pick().is_none());
    }

    #[test]
    fn single_entry_pool_always_returns_it() {
        let pool = ProxyPool::new(vec![entry("http://proxy-a:8080")]);
        for _ in 0..20 {
            assert_eq!(pool.pick().unwrap().uri, "http://proxy-a:8080");
        }
    }

    #[test]
    fn multi_entry_pool_picks_from_all_entries() {
        let pool = ProxyPool::new(vec![
            entry("http://proxy-a:80"),
            entry("http://proxy-b:80"),
            entry("http://proxy-c:80"),
        ]);
        let picks: std::collections::HashSet<String> =
            (0..200).map(|_| pool.pick().unwrap().uri.clone()).collect();
        assert_eq!(picks.len(), 3, "All entries should be reachable");
    }

    #[test]
    fn replace_swaps_pool_entries() {
        let mut pool = ProxyPool::new(vec![entry("http://old:80")]);
        pool.replace(vec![entry("http://new:80")]);
        assert_eq!(pool.pick().unwrap().uri, "http://new:80");
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn replace_with_empty_empties_pool() {
        let mut pool = ProxyPool::new(vec![entry("http://proxy-a:80")]);
        pool.replace(vec![]);
        assert!(pool.pick().is_none());
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn len_reflects_pool_size() {
        let pool = ProxyPool::new(vec![entry("http://proxy-a:80"), entry("http://proxy-b:80")]);
        assert_eq!(pool.len(), 2);
    }

    // ── Edge cases ────────────────────────────────────────────────────────────

    #[test]
    fn duplicate_uris_are_preserved() {
        let pool = ProxyPool::new(vec![entry("http://dup:80"), entry("http://dup:80")]);
        assert_eq!(pool.len(), 2);
        assert_eq!(pool.pick().unwrap().uri, "http://dup:80");
    }

    #[test]
    fn uri_with_credentials_preserved() {
        let raw = "http://user:pass@proxy-a:8080";
        let pool = ProxyPool::new(vec![entry(raw)]);
        assert_eq!(pool.pick().unwrap().uri, raw);
    }

    #[test]
    fn large_pool_pick_does_not_panic() {
        let entries = (0..10_000)
            .map(|i| entry(&format!("http://proxy-{i}:80")))
            .collect();
        let pool = ProxyPool::new(entries);
        assert_eq!(pool.len(), 10_000);
        assert!(pool.pick().is_some());
    }
}

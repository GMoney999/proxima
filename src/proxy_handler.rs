use crate::proxy_pool::SharedPool;
use hudsucker::{
    Body, HttpContext, HttpHandler, RequestOrResponse,
    hyper::{Request, Response},
};

#[derive(Clone)]
pub struct RotatingProxyHandler {
    pub pool: SharedPool,
}

impl RotatingProxyHandler {
    /// Core routing decision, independent of hudsucker's [`HttpContext`]
    /// (which is `#[non_exhaustive]` and cannot be built in tests).
    ///
    /// Picks a random upstream from the pool and tags the request with an
    /// [`UpstreamProxy`] extension, or returns `503 Service Unavailable`
    /// when the pool is empty.
    pub fn route_request(&self, mut req: Request<Body>) -> RequestOrResponse {
        // Clone out the chosen URI so the read lock is released promptly.
        let chosen = self.pool.read().unwrap().pick().map(|e| e.uri.clone());

        match chosen {
            Some(uri) => {
                tracing::debug!(upstream = %uri, method = %req.method(), uri = %req.uri(), "Routing request");
                // Store the upstream choice in a request extension for a
                // downstream connector to pick up.
                req.extensions_mut().insert(UpstreamProxy(uri));
                req.into()
            }
            None => {
                tracing::warn!("Proxy pool is empty — rejecting request");
                Response::builder()
                    .status(503)
                    .body(Body::empty())
                    .expect("static 503 response is always valid")
                    .into()
            }
        }
    }

    /// Response post-processing hook. Currently a pass-through.
    pub fn process_response(&self, res: Response<Body>) -> Response<Body> {
        res
    }
}

impl HttpHandler for RotatingProxyHandler {
    async fn handle_request(
        &mut self,
        _ctx: &HttpContext,
        req: Request<Body>,
    ) -> RequestOrResponse {
        self.route_request(req)
    }

    async fn handle_response(&mut self, _ctx: &HttpContext, res: Response<Body>) -> Response<Body> {
        self.process_response(res)
    }
}

/// Type-safe request extension carrying the chosen upstream URI.
#[derive(Clone, Debug)]
pub struct UpstreamProxy(pub String);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_pool::{ProxyEntry, ProxyPool};
    use std::sync::{Arc, RwLock};

    fn handler_with(uris: &[&str]) -> RotatingProxyHandler {
        let entries = uris
            .iter()
            .map(|u| ProxyEntry { uri: (*u).into() })
            .collect();
        RotatingProxyHandler {
            pool: Arc::new(RwLock::new(ProxyPool::new(entries))),
        }
    }

    fn empty_handler() -> RotatingProxyHandler {
        RotatingProxyHandler {
            pool: Arc::new(RwLock::new(ProxyPool::default())),
        }
    }

    fn get_request() -> Request<Body> {
        Request::builder()
            .uri("http://example.com")
            .body(Body::empty())
            .unwrap()
    }

    #[test]
    fn passes_request_through_with_upstream_extension() {
        let handler = handler_with(&["http://proxy-a:8080"]);
        let req = match handler.route_request(get_request()) {
            RequestOrResponse::Request(r) => r,
            RequestOrResponse::Response(_) => panic!("Expected request to be passed through"),
        };
        let ext = req.extensions().get::<UpstreamProxy>().unwrap();
        assert_eq!(ext.0, "http://proxy-a:8080");
    }

    #[test]
    fn returns_503_when_pool_is_empty() {
        let handler = empty_handler();
        let resp = match handler.route_request(get_request()) {
            RequestOrResponse::Response(r) => r,
            RequestOrResponse::Request(_) => panic!("Expected 503 response"),
        };
        assert_eq!(resp.status(), 503);
    }

    #[test]
    fn response_handler_passes_through_unchanged() {
        let handler = empty_handler();
        let resp = Response::builder().status(200).body(Body::empty()).unwrap();
        assert_eq!(handler.process_response(resp).status(), 200);
    }

    // ── Edge cases ────────────────────────────────────────────────────────────

    #[test]
    fn pool_empty_mid_flight_returns_503() {
        let handler = handler_with(&["http://proxy-a:8080"]);
        // Drain the pool after the handler was created.
        handler.pool.write().unwrap().replace(vec![]);
        let resp = match handler.route_request(get_request()) {
            RequestOrResponse::Response(r) => r,
            RequestOrResponse::Request(_) => panic!("Expected 503 response"),
        };
        assert_eq!(resp.status(), 503);
    }

    #[test]
    fn cloned_handler_shares_pool() {
        let handler = empty_handler();
        let clone = handler.clone();
        clone.pool.write().unwrap().replace(vec![ProxyEntry {
            uri: "http://proxy-a:80".into(),
        }]);
        // The original sees the update through the shared Arc<RwLock<_>>.
        assert_eq!(handler.pool.read().unwrap().len(), 1);
    }

    #[test]
    fn upstream_extension_is_overwritten() {
        let handler = handler_with(&["http://chosen:80"]);
        let mut req = get_request();
        req.extensions_mut()
            .insert(UpstreamProxy("http://stale:80".into()));
        let req = match handler.route_request(req) {
            RequestOrResponse::Request(r) => r,
            RequestOrResponse::Response(_) => panic!("Expected request"),
        };
        assert_eq!(
            req.extensions().get::<UpstreamProxy>().unwrap().0,
            "http://chosen:80"
        );
    }
}

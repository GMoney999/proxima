use crate::proxy_pool::SharedPool;
use hudsucker::{
    Body, HttpContext, HttpHandler, RequestOrResponse,
    hyper::{Request, Response, Uri},
};

#[derive(Clone)]
pub struct RotatingProxyHandler {
    pub pool: SharedPool,
}

impl HttpHandler for RotatingProxyHandler {
    async fn handle_request(
        &mut self,
        _ctx: &HttpContext,
        mut req: Request<Body>,
    ) -> RequestOrResponse {
        let pool = self.pool.read().unwrap();

        match pool.pick() {
            Some(entry) => {
                tracing::debug!(upstream = %entry.uri, method = %req.method(), uri = %req.uri(), "Routing request");

                // Attach the chosen upstream as a custom header that
                // a downstream tower layer / custom connector can read,
                // OR rewrite the request URI to route through upstream.
                //
                // The simplest approach: store upstream choice in a
                // request extension for a custom hyper connector to pick up.
                req.extensions_mut()
                    .insert(UpstreamProxy(entry.uri.clone()));
                req.into()
            }
            None => {
                tracing::warn!("Proxy pool is empty — rejecting request");
                // Return 503 when the pool is exhausted
                let resp = Response::builder().status(503).body(Body::empty()).unwrap();
                resp.into()
            }
        }
    }

    async fn handle_response(&mut self, _ctx: &HttpContext, res: Response<Body>) -> Response<Body> {
        res // pass responses through unchanged
    }
}

/// Type-safe request extension carrying the chosen upstream URI.
#[derive(Clone, Debug)]
pub struct UpstreamProxy(pub String);

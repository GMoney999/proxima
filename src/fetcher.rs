use crate::{Result, proxy_pool::ProxyEntry};

/// Fetches a newline-delimited proxy list from a public URL.
/// Each line should be in `host:port` or `scheme://host:port` format.
pub async fn fetch_proxy_list(url: &str) -> Result<Vec<ProxyEntry>> {
    let text = reqwest::get(url).await?.error_for_status()?.text().await?;

    let entries = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            // If no scheme prefix, assume HTTP proxy
            let uri = if line.contains("://") {
                line.to_string()
            } else {
                format!("http://{line}")
            };
            ProxyEntry { uri }
        })
        .collect();

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mock_list(body: &str) -> (mockito::ServerGuard, String) {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/proxies.txt")
            .with_body(body)
            .create_async()
            .await;
        let url = format!("{}/proxies.txt", server.url());
        (server, url)
    }

    #[tokio::test]
    async fn parses_plain_host_port_list() {
        let (_server, url) = mock_list("proxy-a:8080\nproxy-b:3128\n").await;
        let entries = fetch_proxy_list(&url).await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].uri, "http://proxy-a:8080");
        assert_eq!(entries[1].uri, "http://proxy-b:3128");
    }

    #[tokio::test]
    async fn ignores_comments_and_blank_lines() {
        let (_server, url) =
            mock_list("# comment\n\nproxy-a:8080\n  \n# another comment\nproxy-b:80\n").await;
        let entries = fetch_proxy_list(&url).await.unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn preserves_explicit_schemes() {
        let (_server, url) = mock_list("socks5://proxy-s:1080\nhttp://proxy-h:8080\n").await;
        let entries = fetch_proxy_list(&url).await.unwrap();
        assert_eq!(entries[0].uri, "socks5://proxy-s:1080");
        assert_eq!(entries[1].uri, "http://proxy-h:8080");
    }

    #[tokio::test]
    async fn returns_error_on_non_200() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/proxies.txt")
            .with_status(404)
            .create_async()
            .await;
        let url = format!("{}/proxies.txt", server.url());
        assert!(fetch_proxy_list(&url).await.is_err());
    }

    #[tokio::test]
    async fn empty_vec_on_all_comments() {
        let (_server, url) = mock_list("# only comments\n# nothing here\n").await;
        let entries = fetch_proxy_list(&url).await.unwrap();
        assert!(entries.is_empty());
    }

    // ── Edge cases ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn trims_whitespace_from_entries() {
        let (_server, url) = mock_list("proxy-a:8080  \n").await;
        let entries = fetch_proxy_list(&url).await.unwrap();
        assert_eq!(entries[0].uri, "http://proxy-a:8080");
    }

    #[tokio::test]
    async fn handles_crlf_line_endings() {
        let (_server, url) = mock_list("proxy-a:8080\r\nproxy-b:80\r\n").await;
        let entries = fetch_proxy_list(&url).await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].uri, "http://proxy-a:8080");
        assert!(!entries[0].uri.contains('\r'));
    }

    #[tokio::test]
    async fn empty_body_returns_empty_vec() {
        let (_server, url) = mock_list("").await;
        let entries = fetch_proxy_list(&url).await.unwrap();
        assert!(entries.is_empty());
    }
}

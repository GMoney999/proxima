use crate::{Result, proxy_pool::ProxyEntry};

/// Fetches a newline-delimited proxy list from a public URL.
/// Each line should be in `ip:port` or `scheme://ip:port` format.
pub async fn fetch_proxy_list(url: &str) -> Result<Vec<ProxyEntry>> {
    let text = reqwest::get(url).await?.text().await?;

    let entries = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            // If no scheme prefix, assume HTTP proxy
            let uri = if line.contains("://") {
                line.to_string()
            } else {
                format!("http://{}", line)
            };
            ProxyEntry { uri }
        })
        .collect();

    Ok(entries)
}

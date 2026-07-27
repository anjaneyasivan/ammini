use urlencoding;

/// Build a local proxy URL for a remote URL.
/// `base_url` is the proxy root (e.g. `http://127.0.0.1:12345`).
pub fn local_url_for(base_url: &str, remote_url: &str) -> String {
    let encoded = urlencoding::encode(remote_url);
    format!("{base_url}/url?url={encoded}")
}

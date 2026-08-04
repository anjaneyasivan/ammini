use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, Method, Request, Response, StatusCode},
    response::Response as AxumResponse,
    routing::get,
    Router,
};
use bytes::Bytes;
use grammers_client::Client;
use reqwest::Client as ReqwestClient;
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;

use crate::telegram::client::{Document, VideoDownloadInfo};

/// Chunk size used when downloading a Telegram document. A requested range may start and
/// end mid-chunk; `RangeWindow` trims the excess bytes.
const DOWNLOAD_CHUNK_SIZE: i32 = 512 * 1024;

const URL_REQ_WHITELIST: &[&str] = &[
    "range",
    "accept",
    "accept-language",
    "user-agent",
    "cookie",
    "authorization",
    "referer",
];

const URL_RESP_WHITELIST: &[&str] = &[
    "content-type",
    "content-length",
    "content-range",
    "accept-ranges",
    "etag",
    "last-modified",
    "content-disposition",
];

/// Shared state for the combined proxy server.
pub struct ProxyState {
    pub reqwest_client: ReqwestClient,
    pub video_registry: Arc<Mutex<HashMap<i32, VideoDownloadInfo>>>,
    /// Telegram client used to stream video bytes straight from the network to the player,
    /// with no on-disk cache in between.
    pub telegram_client: Option<Client>,
}

/// Start the axum server on an OS-assigned port.
pub async fn start_server(state: ProxyState) -> anyhow::Result<u16> {
    let app = Router::new()
        .route("/url", get(handle_url).head(handle_url))
        .route("/telegram/{msg_id}", get(handle_telegram).head(handle_telegram))
        .with_state(Arc::new(state));

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let port = addr.port();

    tracing::info!(
        "proxy: listening on http://127.0.0.1:{}/{{url,telegram}}",
        port
    );

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!("proxy: server error: {e}");
        }
    });

    Ok(port)
}

// ───────────────────────────────────────────────────────────────
// Generic remote URL proxy (moved from src/proxy.rs)
// ───────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct UrlParams {
    url: String,
}

async fn handle_url(
    State(state): State<Arc<ProxyState>>,
    Query(params): Query<UrlParams>,
    req: Request<Body>,
) -> AxumResponse {
    let decoded = match urlencoding::decode(&params.url) {
        Ok(d) => d.into_owned(),
        Err(e) => {
            tracing::error!("failed to decode url {}: {e}", params.url);
            return error_response(StatusCode::BAD_REQUEST, "bad url");
        }
    };

    let remote_url = match reqwest::Url::parse(&decoded) {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("failed to parse url {decoded}: {e}");
            return error_response(StatusCode::BAD_REQUEST, "bad url");
        }
    };

    if *req.method() != Method::GET && *req.method() != Method::HEAD {
        return error_response(StatusCode::METHOD_NOT_ALLOWED, "only GET/HEAD allowed");
    }

    tracing::info!("proxy {} -> {}", req.method(), remote_url);

    let mut rb = state
        .reqwest_client
        .request(req.method().clone(), remote_url.clone());
    let mut has_user_agent = false;
    for (name, value) in req.headers() {
        if URL_REQ_WHITELIST.contains(&name.as_str().to_lowercase().as_str()) {
            rb = rb.header(name, value);
            if name.as_str().to_lowercase().as_str() == "user-agent" {
                has_user_agent = true;
            }
        }
    }
    if !has_user_agent {
        rb = rb.header("user-agent", "min-mpv/0.1.0");
    }

    let upstream = match rb.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("proxy request failed for {remote_url}: {e}");
            return error_response(StatusCode::BAD_GATEWAY, "upstream error");
        }
    };

    let status = upstream.status();
    tracing::info!("proxy response {} for {}", status, remote_url);

    let mut resp = Response::builder().status(status);
    for (name, value) in upstream.headers() {
        if URL_RESP_WHITELIST.contains(&name.as_str().to_lowercase().as_str()) {
            resp = resp.header(name, value);
        }
    }

    let body = if req.method() == Method::HEAD {
        Body::empty()
    } else {
        Body::from_stream(upstream.bytes_stream())
    };

    match resp.body(body) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("proxy response build error: {e}");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "proxy error")
        }
    }
}

// ───────────────────────────────────────────────────────────────
// Telegram video proxy
// ───────────────────────────────────────────────────────────────

async fn handle_telegram(
    Path(msg_id): Path<i32>,
    method: Method,
    headers: HeaderMap,
    State(state): State<Arc<ProxyState>>,
) -> AxumResponse {
    tracing::debug!("proxy: telegram request for msg_id={msg_id} method={method}");

    let video = {
        let registry = state.video_registry.lock().await;
        registry.get(&msg_id).cloned()
    };

    let video = match video {
        Some(v) => v,
        None => {
            tracing::warn!("proxy: msg_id={msg_id} not in registry");
            return error_response(StatusCode::NOT_FOUND, "Video not found");
        }
    };

    let total_size = video.size as u64;
    if total_size == 0 {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, "Unknown file size");
    }

    let has_range = headers.contains_key(header::RANGE);
    let (start, end, status, content_range) = match parse_range(&headers, total_size) {
        Some((s, e)) => (
            s,
            e,
            StatusCode::PARTIAL_CONTENT,
            Some(format!("bytes {}-{}/{}", s, e, total_size)),
        ),
        None => (0, total_size - 1, StatusCode::OK, None),
    };

    let content_length = end - start + 1;
    tracing::debug!(
        "proxy: msg_id={msg_id} has_range={has_range} start={start} end={end} len={content_length}"
    );

    if method == Method::HEAD {
        return build_telegram_response(status, content_range, content_length, Body::empty());
    }

    // Stream the requested range straight from Telegram, no disk involved. Every HTTP
    // request spawns its own download starting at the chunk containing `start`; it stops
    // once `end` has been delivered (or the player disconnects).
    let Some(client) = state.telegram_client.clone() else {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, "Telegram client unavailable");
    };

    let stream = telegram_range_stream(client, video.document.clone(), start, end);
    build_telegram_response(status, content_range, content_length, Body::from_stream(stream))
}

fn build_telegram_response(
    status: StatusCode,
    content_range: Option<String>,
    content_length: u64,
    body: Body,
) -> AxumResponse {
    let mut resp = Response::builder().status(status);
    resp = resp.header(header::CONTENT_TYPE, "video/mp4");
    resp = resp.header(header::ACCEPT_RANGES, "bytes");
    if let Some(cr) = content_range {
        resp = resp.header(header::CONTENT_RANGE, cr);
    }
    resp = resp.header(header::CONTENT_LENGTH, content_length.to_string());
    match resp.body(body) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("proxy: failed to build telegram response: {e}");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "Stream error")
        }
    }
}

fn parse_range(headers: &HeaderMap, total_size: u64) -> Option<(u64, u64)> {
    let range_header = headers.get(header::RANGE)?;
    let range_str = range_header.to_str().ok()?;
    let range_str = range_str.strip_prefix("bytes=")?;
    let parts: Vec<&str> = range_str.splitn(2, '-').collect();
    if parts.len() != 2 {
        return None;
    }

    // Suffix range: bytes=-500 means the last 500 bytes.
    if parts[0].is_empty() {
        let suffix_len: u64 = parts[1].parse().ok()?;
        if suffix_len == 0 {
            return None;
        }
        let start = total_size.saturating_sub(suffix_len);
        return Some((start, total_size - 1));
    }

    let start: u64 = parts[0].parse().ok()?;
    let end: u64 = if parts[1].is_empty() {
        total_size - 1
    } else {
        let e: u64 = parts[1].parse().ok()?;
        e.min(total_size - 1)
    };

    if start > end || start >= total_size {
        return None;
    }

    Some((start, end))
}

/// Stream the byte window `[start, end]` of a Telegram document directly from the
/// network to the caller. The download begins at the `DOWNLOAD_CHUNK_SIZE`-aligned chunk
/// containing `start` and stops as soon as `end` has been delivered; dropping the
/// receiver cancels the download.
fn telegram_range_stream(
    client: Client,
    document: Document,
    start: u64,
    end: u64,
) -> ReceiverStream<Result<Bytes, anyhow::Error>> {
    let skip = (start / DOWNLOAD_CHUNK_SIZE as u64) as i32;
    let mut window = RangeWindow::new(start, end, DOWNLOAD_CHUNK_SIZE as u64);
    let mut iter = client
        .iter_download(&document)
        .chunk_size(DOWNLOAD_CHUNK_SIZE)
        .skip_chunks(skip);

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, anyhow::Error>>(8);
    tokio::spawn(async move {
        while !window.done() {
            let chunk = match iter.next().await {
                Ok(Some(c)) => c,
                Ok(None) => break,
                Err(e) => {
                    tracing::error!("proxy: telegram download error: {e}");
                    let _ = tx
                        .send(Err(anyhow::anyhow!("telegram download failed: {e}")))
                        .await;
                    return;
                }
            };
            if let Some(slice) = window.push(chunk) {
                if tx.send(Ok(slice)).await.is_err() {
                    // The player went away; dropping `iter` cancels the Telegram download.
                    return;
                }
            }
        }
    });
    ReceiverStream::new(rx)
}

/// Tracks how much of a Telegram download belongs to the requested byte window so the
/// stream can drop bytes before `start` and stop after `end`.
struct RangeWindow {
    /// Bytes to drop from the first chunk returned by the download iterator.
    skip_prefix: usize,
    /// Bytes already emitted.
    sent: u64,
    /// Total bytes to emit.
    total: u64,
    /// Whether we are still waiting for the first chunk that lies inside the window.
    first: bool,
}

impl RangeWindow {
    fn new(start: u64, end: u64, chunk_size: u64) -> Self {
        let chunk_start = (start / chunk_size) * chunk_size;
        Self {
            skip_prefix: (start - chunk_start) as usize,
            sent: 0,
            total: end - start + 1,
            first: true,
        }
    }

    /// Feed the next chunk from the download iterator and return the slice of it that
    /// belongs to the window (`None` if the chunk lies entirely before `start`).
    fn push(&mut self, chunk: Vec<u8>) -> Option<Bytes> {
        let mut data = Bytes::from(chunk);
        if self.first {
            if self.skip_prefix >= data.len() {
                // Defensive: whole chunk is before the requested window, keep waiting.
                return None;
            }
            self.first = false;
            data = data.slice(self.skip_prefix..);
        }
        let remaining = self.total - self.sent;
        let take = (data.len() as u64).min(remaining) as usize;
        self.sent += take as u64;
        Some(data.slice(..take))
    }

    fn done(&self) -> bool {
        self.sent >= self.total
    }
}

fn error_response(status: StatusCode, msg: &str) -> AxumResponse {
    Response::builder()
        .status(status)
        .body(Body::from(msg.to_string()))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_range_handles_common_formats() {
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, "bytes=0-99".parse().unwrap());
        assert_eq!(parse_range(&headers, 1000), Some((0, 99)));

        headers.insert(header::RANGE, "bytes=100-199".parse().unwrap());
        assert_eq!(parse_range(&headers, 1000), Some((100, 199)));

        headers.insert(header::RANGE, "bytes=500-".parse().unwrap());
        assert_eq!(parse_range(&headers, 1000), Some((500, 999)));

        headers.insert(header::RANGE, "bytes=-100".parse().unwrap());
        assert_eq!(parse_range(&headers, 1000), Some((900, 999)));

        headers.insert(header::RANGE, "bytes=1000-1999".parse().unwrap());
        assert_eq!(parse_range(&headers, 1000), None);

        headers.insert(header::RANGE, "bytes=0-0".parse().unwrap());
        assert_eq!(parse_range(&headers, 1000), Some((0, 0)));
    }

    #[test]
    fn range_window_slices_within_first_chunk() {
        // start=1000, end=1999 both lie inside the first 512 KiB chunk.
        let mut w = RangeWindow::new(1000, 1999, 512 * 1024);
        assert!(!w.done());
        let out = w.push(vec![7u8; 512 * 1024]).unwrap();
        assert_eq!(out.len(), 1000);
        assert!(out.iter().all(|b| *b == 7));
        assert!(w.done());
    }

    #[test]
    fn range_window_consumes_multiple_chunks() {
        // start is 10 bytes before the second chunk boundary; end lands mid-chunk 2.
        let start = 512 * 1024 - 10;
        let end = 1_200_000;
        let mut w = RangeWindow::new(start, end, 512 * 1024);

        // Chunk 0 (bytes 0..512 KiB): only its last 10 bytes belong to the window.
        let out0 = w.push(vec![1u8; 512 * 1024]).unwrap();
        assert_eq!(out0.len(), 10);

        // Chunk 1 is fully inside the window.
        let out1 = w.push(vec![2u8; 512 * 1024]).unwrap();
        assert_eq!(out1.len(), 512 * 1024);

        // Chunk 2 covers the tail of the window.
        let out2 = w.push(vec![3u8; 512 * 1024]).unwrap();
        let expected = end - start + 1 - out0.len() as u64 - out1.len() as u64;
        assert_eq!(out2.len(), expected as usize);
        assert!(w.done());
    }

    #[test]
    fn range_window_chunk_aligned_start() {
        // start exactly on a chunk boundary: the download begins at that chunk.
        let mut w = RangeWindow::new(512 * 1024, 1024 * 1024 + 5, 512 * 1024);
        let out = w.push(vec![2u8; 512 * 1024]).unwrap();
        assert_eq!(out.len(), 512 * 1024);
        let out = w.push(vec![3u8; 512 * 1024]).unwrap();
        assert_eq!(out.len(), 6);
        assert!(w.done());
    }
}

use std::collections::HashMap;
use std::path::PathBuf;
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

use crate::telegram::cache::{self, BlockCache};
use crate::telegram::client::{Document, VideoDownloadInfo};

/// Block size used by the disk cache and the Telegram download iterator. A requested
/// range may start and end mid-block; the streaming code trims the excess bytes.
const DOWNLOAD_CHUNK_SIZE: u64 = cache::BLOCK_SIZE;

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
    /// Telegram client used to stream video bytes to the player through the disk block
    /// cache in `video_cache`.
    pub telegram_client: Option<Client>,
    /// Directory under which per-video block-cache files are created.
    pub cache_dir: PathBuf,
    /// Shared disk block cache: msg_id -> BlockCache. Populated lazily on first request
    /// and evicted together with `video_registry` on chat switch/sign-out.
    pub video_cache: Arc<Mutex<HashMap<i32, BlockCache>>>,
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

    // Serve the requested range through the shared disk block cache. Missing blocks are
    // downloaded from Telegram (deduplicated across concurrent requests) and written to
    // disk; cached blocks are read straight from the file. The telegram client is only
    // needed when a block is actually missing.
    let client = state.telegram_client.clone();
    let cache = {
        let mut caches = state.video_cache.lock().await;
        caches
            .entry(msg_id)
            .or_insert_with(|| {
                let chat_id = video.chat_id;
                BlockCache::new(&state.cache_dir, chat_id, msg_id, total_size)
            })
            .clone()
    };

    let stream = telegram_cache_stream(client, cache, video.document.clone(), start, end);
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

/// Stream byte window `[start, end]` of a Telegram document through the shared disk
/// block cache.
///
/// Blocks already on disk are read straight from the cache file. Missing blocks are
/// downloaded from Telegram via `cache.ensure` — the first request for a block owns the
/// download, concurrent requests for the same block wait and then read from disk
/// (cooperative fill). The stream stops once `end` has been delivered; dropping the
/// receiver stops the loop, which cancels any in-progress download.
fn telegram_cache_stream(
    client: Option<Client>,
    cache: BlockCache,
    document: Document,
    start: u64,
    end: u64,
) -> ReceiverStream<Result<Bytes, anyhow::Error>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, anyhow::Error>>(8);
    tokio::spawn(async move {
        let total_size = cache.total_size();
        let blocks = cache::blocks_between(start, end);
        let first_block = *blocks.start();
        let last_block = *blocks.end();
        // Bytes to trim off the first/last block so the emitted bytes are exactly
        // the requested window.
        let trim_prefix = (start - first_block * DOWNLOAD_CHUNK_SIZE) as usize;
        let trim_suffix =
            (((last_block + 1) * DOWNLOAD_CHUNK_SIZE).min(total_size) - 1 - end) as usize;

        for block in first_block..=last_block {
            let result = loop {
                match cache.read_block(block).await {
                    Ok(Some(bytes)) => break Ok(bytes),
                    Ok(None) => {}
                    Err(e) => break Err(anyhow::anyhow!("cache read error: {e}")),
                }
                // Block is not on disk yet. Ensure it is downloaded (or wait for the
                // in-flight owner) and then retry the read.
                let Some(client) = client.clone() else {
                    break Err(anyhow::anyhow!("telegram client unavailable"));
                };
                let doc = document.clone();
                if let Err(e) = cache
                    .ensure(block, async move { download_block(&client, &doc, block).await })
                    .await
                {
                    break Err(e);
                }
            };

            let bytes = match result {
                Ok(b) => b,
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                    return;
                }
            };

            // Trim to the requested window. The final block may be partial, so
            // `end_idx` is bounded by the actual block length.
            let start_idx = if block == first_block { trim_prefix } else { 0 };
            let end_idx = bytes
                .len()
                .saturating_sub(if block == last_block { trim_suffix } else { 0 });
            if start_idx < end_idx {
                if tx.send(Ok(bytes.slice(start_idx..end_idx))).await.is_err() {
                    return; // player went away
                }
            }
        }
    });
    ReceiverStream::new(rx)
}

/// Download the whole 512 KiB block `block` from Telegram (the final block of the file
/// may be shorter if the file size is not a multiple of the block size). Returns only
/// this block's bytes, not a covering chunk.
async fn download_block(
    client: &Client,
    document: &Document,
    block: u64,
) -> Result<Vec<u8>, anyhow::Error> {
    let skip = u32::try_from(block).map_err(|_| anyhow::anyhow!("block {block} out of range"))?;
    // Telegram documents are downloaded as fixed "chunks"; skipping `skip` chunks
    // positions the iterator exactly at block `block`.
    let mut iter = client
        .iter_download(document)
        .chunk_size(DOWNLOAD_CHUNK_SIZE as i32)
        .skip_chunks(skip as i32);
    match iter.next().await {
        Ok(Some(chunk)) => Ok(chunk),
        Ok(None) => Err(anyhow::anyhow!(
            "block {block}: telegram download returned no data"
        )),
        Err(e) => Err(anyhow::anyhow!("block {block}: telegram download failed: {e}")),
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

    /// Trim a full block's bytes down to the requested range window. `block` is the block
    /// index, `start`/`end` the requested byte window, `total_size` the file size.
    fn trim_block(block: u64, first_block: u64, last_block: u64, trim_prefix: usize, trim_suffix: usize, bytes: &[u8]) -> &[u8] {
        let start_idx = if block == first_block { trim_prefix } else { 0 };
        let end_idx = bytes
            .len()
            .saturating_sub(if block == last_block { trim_suffix } else { 0 });
        &bytes[start_idx.min(bytes.len())..end_idx.min(bytes.len())]
    }

    /// The window edges of a byte range, mirroring the stream's computation.
    fn window_edges(start: u64, end: u64, total_size: u64) -> (u64, u64, usize, usize) {
        let r = cache::blocks_between(start, end);
        let first_block = *r.start();
        let last_block = *r.end();
        let trim_prefix = (start - first_block * DOWNLOAD_CHUNK_SIZE) as usize;
        let trim_suffix = (((last_block + 1) * DOWNLOAD_CHUNK_SIZE).min(total_size) - 1 - end) as usize;
        (first_block, last_block, trim_prefix, trim_suffix)
    }

    #[test]
    fn trim_block_slices_within_first_chunk() {
        // start=1000, end=1999 both lie inside the first 512 KiB block.
        let (fb, lb, tp, ts) = window_edges(1000, 1999, 512 * 1024);
        assert_eq!((fb, lb), (0, 0));
        let out = trim_block(fb, fb, lb, tp, ts, &[7u8; 512 * 1024]);
        assert_eq!(out.len(), 1000);
        assert!(out.iter().all(|b| *b == 7));
    }

    #[test]
    fn trim_block_covers_mid_block_fully() {
        // start is 10 bytes before the second block boundary; end lands mid-block 2.
        // The file is exactly 3 full blocks, so block 2 is a full 512 KiB block.
        let start = 512 * 1024 - 10;
        let end = 1_200_000;
        let total_size = 3 * 512 * 1024;
        let (fb, lb, tp, ts) = window_edges(start, end, total_size);
        assert_eq!((fb, lb), (0, 2));

        // Block 0: only the last 10 bytes belong to the window.
        let out0 = trim_block(fb, fb, lb, tp, ts, &[1u8; 512 * 1024]);
        assert_eq!(out0.len(), 10);

        // Block 1 is fully inside the window.
        let out1 = trim_block(fb + 1, fb, lb, tp, ts, &[2u8; 512 * 1024]);
        assert_eq!(out1.len(), 512 * 1024);

        // Block 2 covers the tail of the window.
        let out2 = trim_block(fb + 2, fb, lb, tp, ts, &[3u8; 512 * 1024]);
        let expected = end - start + 1 - out0.len() as u64 - out1.len() as u64;
        assert_eq!(out2.len(), expected as usize);
    }

    #[test]
    fn trim_block_chunk_aligned_start() {
        // start exactly on a block boundary: the first block is fully inside.
        let start = 512 * 1024;
        let end = 1024 * 1024 + 5;
        let total_size = 2 * 1024 * 1024;
        let (fb, lb, tp, ts) = window_edges(start, end, total_size);
        assert_eq!((fb, lb), (1, 2));
        let out1 = trim_block(1, fb, lb, tp, ts, &[2u8; 512 * 1024]);
        assert_eq!(out1.len(), 512 * 1024);
        let out2 = trim_block(2, fb, lb, tp, ts, &[3u8; 512 * 1024]);
        assert_eq!(out2.len(), 6);
    }

    #[test]
    fn trim_block_final_partial_block() {
        // File is not a multiple of the block size; the last block is shorter.
        let total_size = 512 * 1024 + 100;
        let start = 512 * 1024;
        let end = total_size - 1;
        let (fb, lb, tp, ts) = window_edges(start, end, total_size);
        assert_eq!((fb, lb), (1, 1));
        // The final block on disk is only 100 bytes (file size - 512 KiB).
        let out = trim_block(1, fb, lb, tp, ts, &[9u8; 100]);
        assert_eq!(out.len(), 100);
    }
}

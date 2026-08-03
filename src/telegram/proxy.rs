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

use crate::telegram::cache::{DocCache, DocCacheManager};
use crate::telegram::client::{Document, VideoDownloadInfo};

const RANGE_DOWNLOAD_CHUNK_SIZE: i32 = 512 * 1024;

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
    pub cache_manager: Arc<Mutex<DocCacheManager>>,
    pub video_registry: Arc<Mutex<HashMap<i32, VideoDownloadInfo>>>,
    /// Optional Telegram client used to fetch ranges on demand when the cache doesn't have them.
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

    tracing::info!("proxy: listening on http://127.0.0.1:{}/{{url,telegram}}", port);

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
    tracing::debug!("proxy: telegram request for msg_id={msg_id}");

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

    if method == Method::HEAD {
        return build_telegram_response(status, content_range, content_length, Body::empty());
    }

    let cache = {
        let mut cm = state.cache_manager.lock().await;
        match cm.get_or_create(video.chat_id, msg_id, total_size).await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("proxy: failed to create cache: {e}");
                return error_response(StatusCode::INTERNAL_SERVER_ERROR, "Cache error");
            }
        }
    };

    // If the requested range isn't in the cache yet, kick off an on-demand download so the
    // stream doesn't block forever waiting for the sequential background download to reach it.
    if !cache.is_range_downloaded(start, end).await {
        if let Some(client) = state.telegram_client.clone() {
            let document = video.document.clone();
            let dl_cache = cache.clone();
            tokio::spawn(async move {
                download_range(client, document, dl_cache, start).await;
            });
        }
    }

    let stream = cache_stream(cache, start, end);
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

/// Stream a byte range from the cache as it becomes available.
fn cache_stream(
    cache: Arc<DocCache>,
    start: u64,
    end: u64,
) -> ReceiverStream<Result<Bytes, anyhow::Error>> {
    const CHUNK_LIMIT: u64 = 64 * 1024;

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, anyhow::Error>>(8);
    tokio::spawn(async move {
        let mut offset = start;
        while offset <= end {
            let target_end = (offset + CHUNK_LIMIT).min(end + 1);
            if let Err(e) = cache.wait_for_range(offset, target_end).await {
                let _ = tx.send(Err(e)).await;
                return;
            }
            let available = cache
                .contiguous_available_from(offset, end)
                .await
                .min(CHUNK_LIMIT)
                .min(end - offset + 1);
            if available == 0 {
                return;
            }
            match cache.read_at(offset, available).await {
                Ok(data) => {
                    let len = data.len() as u64;
                    if tx.send(Ok(Bytes::from(data))).await.is_err() {
                        return;
                    }
                    offset += len;
                }
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                    return;
                }
            }
        }
    });
    ReceiverStream::new(rx)
}

/// Download a specific byte range from Telegram and write it into the cache.
/// The iterator is positioned at the chunk containing `start`; downloaded bytes before `start`
/// are still written to the cache because Telegram returns fixed chunk boundaries.
async fn download_range(client: Client, document: Document, cache: Arc<DocCache>, start: u64) {
    let chunk_size = RANGE_DOWNLOAD_CHUNK_SIZE;
    let skip = (start / chunk_size as u64) as i32;
    let mut iter = client
        .iter_download(&document)
        .chunk_size(chunk_size)
        .skip_chunks(skip);

    let mut offset = skip as u64 * chunk_size as u64;
    loop {
        let chunk = match iter.next().await {
            Ok(Some(c)) => c,
            Ok(None) => break,
            Err(e) => {
                tracing::error!("proxy: range download error at offset {}: {}", offset, e);
                break;
            }
        };
        if let Err(e) = cache.write_at(offset, &chunk).await {
            tracing::error!("proxy: cache write error at offset {}: {}", offset, e);
            break;
        }
        offset += chunk.len() as u64;
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
    use std::time::Duration;
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn cache_stream_yields_while_downloading() {
        let dir = std::env::temp_dir().join(format!("min-mpv-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cache = DocCache::create(&dir, 1, 100, 1024).await.unwrap();

        let stream = cache_stream(cache.clone(), 0, 9);
        let collector = tokio::spawn(async move {
            let mut bytes = Vec::new();
            let mut stream = stream;
            while let Some(chunk) = stream.next().await {
                bytes.extend_from_slice(&chunk.unwrap());
            }
            bytes
        });

        // Simulate sequential download: first half, then the rest.
        cache.write_at(0, &[0, 1, 2, 3, 4]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        cache.write_at(5, &[5, 6, 7, 8, 9]).await.unwrap();

        let result = tokio::time::timeout(Duration::from_secs(5), collector)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn cache_stream_reads_non_sequential_range() {
        let dir = std::env::temp_dir().join(format!("min-mpv-test-ns-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cache = DocCache::create(&dir, 1, 200, 1024).await.unwrap();

        // Write the tail of the file first, as an on-demand range download would.
        cache.write_at(100, &[10, 11, 12, 13, 14]).await.unwrap();

        let stream = cache_stream(cache.clone(), 100, 104);
        let result = tokio::time::timeout(Duration::from_secs(5), async move {
            let mut bytes = Vec::new();
            let mut stream = stream;
            while let Some(chunk) = stream.next().await {
                bytes.extend_from_slice(&chunk.unwrap());
            }
            bytes
        })
        .await
        .unwrap();

        assert_eq!(result, vec![10, 11, 12, 13, 14]);
        std::fs::remove_dir_all(&dir).ok();
    }

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
}

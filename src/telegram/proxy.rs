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
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use crate::telegram::cache::{DocCache, DocCacheManager};
use crate::telegram::client::VideoDownloadInfo;
use grammers_client::Client as TelegramClient;
use reqwest::Client as ReqwestClient;

const MAX_CHUNK_SIZE: i32 = 512 * 1024;
const SEEK_THRESHOLD: u64 = 2 * 1024 * 1024;

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
    pub telegram_client: TelegramClient,
    pub cache_manager: Arc<Mutex<DocCacheManager>>,
    pub video_registry: Arc<Mutex<HashMap<i32, VideoDownloadInfo>>>,
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

    let (start, end) = match parse_range(&headers, total_size) {
        Some(r) => r,
        None => {
            return full_not_ready(&video, total_size, &state).await;
        }
    };

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

    if let Err(e) = ensure_range(&cache, &video, start, end, &state).await {
        tracing::error!("proxy: failed to ensure range: {e}");
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, "Download error");
    }

    let length = end - start + 1;
    let data = match cache.read_at(start, length).await {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("proxy: failed to read cache: {e}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "Read error");
        }
    };

    Response::builder()
        .status(StatusCode::PARTIAL_CONTENT)
        .header(header::CONTENT_TYPE, "video/mp4")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{}", start, end, total_size),
        )
        .header(header::CONTENT_LENGTH, data.len().to_string())
        .body(Body::from(data))
        .unwrap()
}

fn parse_range(headers: &HeaderMap, total_size: u64) -> Option<(u64, u64)> {
    let range_header = headers.get(header::RANGE)?;
    let range_str = range_header.to_str().ok()?;
    let range_str = range_str.strip_prefix("bytes=")?;
    let parts: Vec<&str> = range_str.splitn(2, '-').collect();
    if parts.len() != 2 {
        return None;
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

async fn full_not_ready(
    video: &VideoDownloadInfo,
    total_size: u64,
    state: &ProxyState,
) -> AxumResponse {
    let cache = {
        let mut cm = state.cache_manager.lock().await;
        match cm.get_or_create(video.chat_id, video.msg_id, total_size).await {
            Ok(c) => c,
            Err(_) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, "Cache error"),
        }
    };

    if let Err(e) = ensure_range(&cache, video, 0, total_size - 1, state).await {
        tracing::error!("proxy: full download failed: {e}");
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, "Download error");
    }

    let data = match cache.read_at(0, total_size).await {
        Ok(d) => d,
        Err(_) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, "Read error"),
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "video/mp4")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, data.len().to_string())
        .body(Body::from(data))
        .unwrap()
}

async fn ensure_range(
    cache: &Arc<DocCache>,
    video: &VideoDownloadInfo,
    start: u64,
    end: u64,
    state: &ProxyState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if cache.is_range_downloaded(start, end).await {
        return Ok(());
    }

    let watermark = cache.watermark();

    if start >= watermark && start.saturating_sub(watermark) < SEEK_THRESHOLD {
        tracing::debug!(
            "proxy: waiting for watermark to reach {} (current={})",
            end + 1,
            watermark
        );
        cache.wait_for_watermark(end + 1).await?;
        return Ok(());
    }

    let prefetch_end = (end + 1).max(start + SEEK_THRESHOLD).min(video.size as u64);

    tracing::info!(
        "proxy: on-demand download msg_id={} bytes {}..={}",
        video.msg_id,
        start,
        prefetch_end - 1
    );

    let skip = (start / MAX_CHUNK_SIZE as u64) as i32;
    let mut iter = state
        .telegram_client
        .iter_download(&video.document)
        .chunk_size(MAX_CHUNK_SIZE)
        .skip_chunks(skip);

    let mut offset = start;
    while offset < prefetch_end {
        match iter.next().await? {
            Some(chunk) => {
                cache.write_at(offset, &chunk).await?;
                offset += chunk.len() as u64;
            }
            None => break,
        }
    }

    Ok(())
}

fn error_response(status: StatusCode, msg: &str) -> AxumResponse {
    Response::builder()
        .status(status)
        .body(Body::from(msg.to_string()))
        .unwrap()
}

use serde::Deserialize;

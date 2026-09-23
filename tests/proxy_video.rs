//! Offline integration tests for the Telegram video proxy: a real axum server with a
//! fake [`VideoSource`] and a temp disk cache. No Telegram network access, no mpv, no
//! HEVC fixture — the proxy layer is codec-agnostic (HEVC only matters to the live
//! `tests/hevc_proxy.rs`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use min_mpv::telegram::cache::BLOCK_SIZE;
use min_mpv::telegram::client::{Document, VideoDownloadInfo, VideoSource};
use min_mpv::telegram::proxy::{ProxyState, start_server};
use tokio::sync::Mutex;

/// Deterministic byte at file offset `i`, shared by the fake source and assertions.
fn byte_at(i: u64) -> u8 {
    ((i * 7 + 3) % 251) as u8
}

/// Fake [`VideoSource`] serving pattern bytes per block and counting downloads, with
/// optional per-block delay (used by the prefetch test) and failure injection.
#[derive(Default)]
struct FakeSource {
    total_size: u64,
    downloads: Arc<AtomicU64>,
    fail_block: Option<u64>,
    delay: Option<Duration>,
}

impl VideoSource for FakeSource {
    fn download_block<'a>(
        &'a self,
        _document: &'a Document,
        block: u64,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<u8>, anyhow::Error>> + Send + 'a>,
    > {
        Box::pin(async move {
            if self.fail_block == Some(block) {
                return Err(anyhow::anyhow!("fake failure on block {block}"));
            }
            if let Some(d) = self.delay {
                tokio::time::sleep(d).await;
            }
            self.downloads.fetch_add(1, Ordering::SeqCst);
            let (s, e) = min_mpv::telegram::cache::block_extent(block, self.total_size);
            Ok((s..=e).map(byte_at).collect())
        })
    }
}

fn dummy_document() -> Document {
    Document::from_raw_media(grammers_client::tl::types::MessageMediaDocument {
        nopremium: false,
        spoiler: false,
        video: false,
        round: false,
        voice: false,
        document: None,
        alt_documents: None,
        video_cover: None,
        video_timestamp: None,
        ttl_seconds: None,
    })
}

/// Start a proxy server with one registered video (msg_id 1) of `total_size` bytes.
/// Returns (port, cache dir) so tests can clean up.
async fn start_test_server(total_size: u64, source: FakeSource) -> (u16, PathBuf) {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let cache_dir = std::env::temp_dir().join(format!(
        "min-mpv-proxy-test-{}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&cache_dir);

    let mut registry = HashMap::new();
    registry.insert(
        1,
        VideoDownloadInfo {
            msg_id: 1,
            chat_id: 1,
            document: dummy_document(),
            size: total_size as usize,
        },
    );
    let state = ProxyState {
        reqwest_client: reqwest::Client::new(),
        video_registry: Arc::new(Mutex::new(registry)),
        video_source: Arc::new(source),
        cache_dir: cache_dir.clone(),
        video_cache: Arc::new(Mutex::new(HashMap::new())),
    };
    let port = start_server(state).await.expect("proxy server failed to start");
    (port, cache_dir)
}

fn proxy_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/telegram/1")
}

async fn get_range(client: &reqwest::Client, url: &str, range: Option<&str>) -> reqwest::Response {
    let mut req = client.get(url);
    if let Some(r) = range {
        req = req.header("Range", r);
    }
    req.send().await.expect("request failed")
}

#[tokio::test]
async fn full_request_serves_every_byte() {
    let total = 2 * BLOCK_SIZE + 123; // spans three blocks, last one partial
    let (port, dir) = start_test_server(total, FakeSource { total_size: total, ..Default::default() }).await;
    let res = get_range(&reqwest::Client::new(), &proxy_url(port), None).await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()["accept-ranges"], "bytes");
    let body = res.bytes().await.unwrap();
    assert_eq!(body.len() as u64, total);
    let expected: Vec<u8> = (0..total).map(byte_at).collect();
    assert_eq!(body.as_ref(), expected.as_slice());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn range_request_returns_the_requested_window() {
    let total = 3 * BLOCK_SIZE;
    let (port, dir) = start_test_server(total, FakeSource { total_size: total, ..Default::default() }).await;
    let res = get_range(
        &reqwest::Client::new(),
        &proxy_url(port),
        Some("bytes=100-200"),
    )
    .await;
    assert_eq!(res.status(), 206);
    assert_eq!(
        res.headers()["content-range"],
        format!("bytes 100-200/{total}")
    );
    let body = res.bytes().await.unwrap();
    assert_eq!(body.len(), 101);
    assert_eq!(body[0], byte_at(100));
    assert_eq!(body[100], byte_at(200));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn suffix_range_returns_the_tail() {
    let total = 3 * BLOCK_SIZE;
    let (port, dir) = start_test_server(total, FakeSource { total_size: total, ..Default::default() }).await;
    let res = get_range(&reqwest::Client::new(), &proxy_url(port), Some("bytes=-500")).await;
    let body = res.bytes().await.unwrap();
    assert_eq!(body.len(), 500);
    assert_eq!(body[0], byte_at(total - 500));
    assert_eq!(body[499], byte_at(total - 1));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn cached_blocks_are_not_downloaded_twice() {
    let total = 3 * BLOCK_SIZE;
    let source = FakeSource { total_size: total, ..Default::default() };
    let downloads = source.downloads.clone();
    let (port, dir) = start_test_server(total, source).await;
    let client = reqwest::Client::new();

    // First request lives inside block 0 -> exactly one download.
    let res = get_range(&client, &proxy_url(port), Some("bytes=0-1023")).await;
    assert_eq!(res.status(), 206);
    assert_eq!(res.bytes().await.unwrap().len(), 1024);
    assert_eq!(downloads.load(Ordering::SeqCst), 1);

    // Second request hits the same block -> served from disk, no new download.
    let res = get_range(&client, &proxy_url(port), Some("bytes=500000-500099")).await;
    assert_eq!(res.status(), 206);
    assert_eq!(res.bytes().await.unwrap().len(), 100);
    assert_eq!(downloads.load(Ordering::SeqCst), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn concurrent_same_block_requests_download_once() {
    let total = 3 * BLOCK_SIZE;
    let source = FakeSource {
        delay: Some(Duration::from_millis(80)),
        total_size: total,
        ..Default::default()
    };
    let downloads = source.downloads.clone();
    let (port, dir) = start_test_server(total, source).await;
    let client = reqwest::Client::new();

    // Two overlapping requests for the same single block: the first claims the
    // download, the second waits and reads from disk.
    let url = proxy_url(port);
    let (a, b) = tokio::join!(
        get_range(&client, &url, Some("bytes=0-1023")),
        get_range(&client, &url, Some("bytes=1000-2023")),
    );
    assert_eq!(a.bytes().await.unwrap().len(), 1024);
    assert_eq!(b.bytes().await.unwrap().len(), 1024);
    assert_eq!(
        downloads.load(Ordering::SeqCst),
        1,
        "same block must be downloaded exactly once"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn unknown_video_returns_404() {
    let (port, dir) = start_test_server(1024, FakeSource::default()).await;
    let res = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/telegram/999"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn failed_block_aborts_the_body() {
    let total = BLOCK_SIZE;
    let source = FakeSource {
        fail_block: Some(0),
        total_size: total,
        ..Default::default()
    };
    let (port, dir) = start_test_server(total, source).await;
    let res = get_range(&reqwest::Client::new(), &proxy_url(port), None).await;
    assert_eq!(res.status(), 200); // headers are sent before the stream error
    assert!(
        res.bytes().await.is_err(),
        "a failed block must abort the response body"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn head_request_has_no_body() {
    let total = BLOCK_SIZE;
    let (port, dir) = start_test_server(total, FakeSource { total_size: total, ..Default::default() }).await;
    let res = reqwest::Client::new()
        .head(&proxy_url(port))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()["content-length"], total.to_string());
    assert_eq!(res.bytes().await.unwrap().len(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn prefetch_downloads_future_blocks_in_parallel() {
    // The request needs blocks 0-2, so blocks 1-4 are prefetched while 0 streams.
    let total = 6 * BLOCK_SIZE;
    let end = 3 * BLOCK_SIZE - 1;
    let source = FakeSource {
        delay: Some(Duration::from_millis(150)),
        total_size: total,
        ..Default::default()
    };
    let downloads = source.downloads.clone();
    let (port, dir) = start_test_server(total, source).await;

    let started = std::time::Instant::now();
    let res = get_range(
        &reqwest::Client::new(),
        &proxy_url(port),
        Some(&format!("bytes=0-{end}")),
    )
    .await;
    assert_eq!(res.bytes().await.unwrap().len() as u64, end + 1);
    let elapsed = started.elapsed();

    // Sequential fetches of blocks 0-2 would need ~3 * 150 ms; prefetch overlaps them,
    // so the whole request finishes in roughly one delay. The window is bounded by the
    // request's last block, so exactly three blocks are fetched.
    assert_eq!(
        downloads.load(Ordering::SeqCst),
        3,
        "all three requested blocks should be fetched"
    );
    assert!(
        elapsed < Duration::from_millis(350),
        "fetch took {elapsed:?}, expected parallel downloads"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
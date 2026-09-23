//! End-to-end test that finds the first HEVC video in the user's Telegram chats and verifies
//! that the local proxy serves it correctly with range requests.
//!
//! This test needs a valid saved Telegram session and a chat containing at least one HEVC video.
//! If either is missing it prints a clear message and exits without failing.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use min_mpv::telegram::cache::BlockCache;
use min_mpv::telegram::client::{find_first_hevc_video, is_hevc_video, TelegramClient};
use min_mpv::telegram::config::TelegramConfig;
use min_mpv::telegram::proxy::{start_server, ProxyState};
use min_mpv::telegram::session;

#[tokio::test]
async fn find_first_hevc_video_and_test_proxy_range() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("min_mpv=debug"))
        .init();

    let config = match TelegramConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Skipping test: no Telegram config in environment: {e}");
            return;
        }
    };

    let session = match session::load_or_create_session().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Skipping test: failed to load session: {e}");
            return;
        }
    };

    let client = match TelegramClient::connect(&config, session).await {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("Skipping test: failed to connect to Telegram: {e}");
            return;
        }
    };

    let authorized = match client.is_authorized().await {
        Ok(true) => true,
        Ok(false) => {
            eprintln!("Skipping test: Telegram session is not authorized");
            return;
        }
        Err(e) => {
            eprintln!("Skipping test: failed to check authorization: {e}");
            return;
        }
    };
    assert!(authorized);

    let video = match find_first_hevc_video(&client).await {
        Ok(Some(v)) => v,
        Ok(None) => {
            eprintln!("Skipping test: no HEVC video found in any chat");
            return;
        }
        Err(e) => {
            panic!("Failed to search for HEVC video: {e}");
        }
    };

    let name = video.document.name().unwrap_or("(no name)");
    println!("Found HEVC video: msg_id={} size={} name={}", video.msg_id, video.size, name);
    assert!(is_hevc_video(&video));

    let mut registry = HashMap::new();
    registry.insert(video.msg_id, video.clone());
    let video_registry = Arc::new(Mutex::new(registry));
    let video_cache: Arc<Mutex<HashMap<i32, BlockCache>>> = Arc::new(Mutex::new(HashMap::new()));

    // Telegram videos are served through the shared disk block cache.
    let proxy_state = ProxyState {
        reqwest_client: reqwest::Client::new(),
        video_registry: video_registry.clone(),
        telegram_client: Some(client.clone_inner()),
        cache_dir: std::env::temp_dir().join("min-mpv-test-telegram-cache"),
        video_cache: video_cache.clone(),
    };

    let port = start_server(proxy_state).await.expect("proxy server failed to start");
    let base = format!("http://127.0.0.1:{}/telegram/{}", port, video.msg_id);
    let http = reqwest::Client::new();

    // 1. HEAD request should advertise range support and the full content length.
    let head = http
        .head(&base)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .expect("HEAD request failed");
    assert_eq!(head.status(), reqwest::StatusCode::OK);
    assert_eq!(head.headers().get("accept-ranges").unwrap(), "bytes");
    let content_length: u64 = head
        .headers()
        .get("content-length")
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(content_length, video.size as u64);
    assert!(head.text().await.unwrap().is_empty());

    // 2. Range request for the first chunk.
    let first_chunk_size = 64 * 1024u64;
    let range_end = (first_chunk_size - 1).min(video.size as u64 - 1);
    let res = http
        .get(&base)
        .header("Range", format!("bytes=0-{}", range_end))
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .expect("first range request failed");
    assert_eq!(res.status(), reqwest::StatusCode::PARTIAL_CONTENT);
    let body = res.bytes().await.unwrap();
    assert_eq!(body.len() as u64, range_end + 1);

    // 3. Range request for the tail of the file. This is the critical case for HEVC files where
    // the moov atom is often at the end and the player seeks there before playback.
    let tail_len = 256 * 1024u64;
    let tail_start = video.size as u64 - tail_len;
    let res = http
        .get(&base)
        .header("Range", format!("bytes={}-", tail_start))
        .timeout(Duration::from_secs(120))
        .send()
        .await
        .expect("tail range request failed");
    assert_eq!(res.status(), reqwest::StatusCode::PARTIAL_CONTENT);
    let content_range = res.headers().get("content-range").unwrap().to_str().unwrap();
    let expected_range = format!("bytes {}-{}/{}", tail_start, video.size - 1, video.size);
    assert_eq!(content_range, expected_range);
    let body = res.bytes().await.unwrap();
    assert!(!body.is_empty());
    assert_eq!(body.len() as u64, tail_len);

    // 4. Repeat the same tail range request. The blocks are now on disk from step 3,
    // so this must be served from the cache (and therefore complete quickly).
    let res = reqwest::Client::new()
        .get(&base)
        .header("Range", format!("bytes={}-", tail_start))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .expect("cached tail range request failed");
    assert_eq!(res.status(), reqwest::StatusCode::PARTIAL_CONTENT);
    let body = res.bytes().await.unwrap();
    assert_eq!(body.len() as u64, tail_len);
}

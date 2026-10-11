//! End-to-end test that finds the first HEVC video in the user's Telegram chats and verifies
//! that the local proxy serves it correctly with range requests.
//!
//! This test needs a valid saved Telegram session and a chat containing at least one HEVC video.
//! If either is missing it prints a clear message and exits without failing.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use ammini::telegram::cache::BlockCache;
use ammini::telegram::client::{
    GrammersVideoSource, TelegramClient, find_first_hevc_video, is_hevc_video,
};
use ammini::telegram::config::TelegramConfig;
use ammini::telegram::proxy::{ProxyState, start_server};
use ammini::telegram::session;

#[tokio::test]
async fn find_first_hevc_video_and_test_proxy_range() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("ammini=debug"))
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

    let (client, _updates) = match TelegramClient::connect(&config, session).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Skipping test: failed to connect to Telegram: {e}");
            return;
        }
    };
    let client = Arc::new(client);

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

    let (peer, video) = match find_first_hevc_video(&client).await {
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
    println!(
        "Found HEVC video: msg_id={} size={} name={}",
        video.msg_id, video.size, name
    );
    assert!(is_hevc_video(&video));

    // The recent-Telegram replay path refetches with the stored peer (access hash);
    // verify it recovers the same document.
    let by_id = ammini::telegram::client::fetch_video_info(client.inner(), peer, video.msg_id)
        .await
        .expect("failed to refetch video by id");
    assert_eq!(by_id.msg_id, video.msg_id);
    assert_eq!(by_id.size, video.size);

    let mut registry = HashMap::new();
    registry.insert(video.msg_id, video.clone());
    let video_registry = Arc::new(Mutex::new(registry));
    let video_cache: Arc<Mutex<HashMap<i32, BlockCache>>> = Arc::new(Mutex::new(HashMap::new()));

    // Telegram videos are served through the shared disk block cache. Start from an
    // empty cache dir: the dir is reused between runs, and a warm cache would let the
    // checks below pass without ever touching Telegram (notably the expired-reference
    // case, whose whole point is the download path).
    let cache_dir = std::env::temp_dir().join("ammini-test-telegram-cache");
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir).expect("failed to create test cache dir");
    let proxy_state = ProxyState {
        reqwest_client: reqwest::Client::new(),
        video_registry: video_registry.clone(),
        video_source: Arc::new(GrammersVideoSource::new(
            client.clone_inner(),
            video_registry.clone(),
        )),
        cache_dir: cache_dir.clone(),
        video_cache: video_cache.clone(),
    };

    let port = start_server(proxy_state)
        .await
        .expect("proxy server failed to start");
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
    let content_range = res
        .headers()
        .get("content-range")
        .unwrap()
        .to_str()
        .unwrap();
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

    // 5. An expired `file_reference` must not kill playback. Telegram expires the
    // reference on the document, and grammers-client never refreshes it, so a
    // long-lived registry entry eventually fails every block download. Poison the
    // document to emulate that and assert the source refetches the message and retries:
    // without the refresh the range below returns a 206 with an empty body.
    {
        let mut stale = video.clone();
        match stale.document.raw.document.as_mut() {
            Some(grammers_client::tl::enums::Document::Document(d)) => {
                d.file_reference = vec![0xAB; d.file_reference.len()];
            }
            other => panic!("unexpected document variant: {other:?}"),
        }
        video_registry.lock().await.insert(stale.msg_id, stale);

        // A small byte range well past anything the previous steps cached, so
        // this block is missing and the download path runs.
        let probe_start = video.size as u64 / 2;
        let probe_end = (probe_start + 64 * 1024 - 1).min(video.size as u64 - 1);
        let res = http
            .get(&base)
            .header("Range", format!("bytes={probe_start}-{probe_end}"))
            .timeout(Duration::from_secs(60))
            .send()
            .await
            .expect("probe range request failed");
        assert_eq!(res.status(), reqwest::StatusCode::PARTIAL_CONTENT);
        let body = res.bytes().await.unwrap();
        assert_eq!(
            body.len() as u64,
            probe_end + 1 - probe_start,
            "expired file reference was not refreshed: short/empty body"
        );

        // The refreshed document must have replaced the poisoned one in the registry,
        // so a repeat request is served from the cache rather than re-fetching.
        let refreshed = video_registry
            .lock()
            .await
            .get(&video.msg_id)
            .unwrap()
            .clone();
        assert_ne!(refreshed.document.raw, video.document.raw);
    }
}

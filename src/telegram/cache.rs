//! Disk-backed block cache for Telegram video streaming.
//!
//! Videos are cached as fixed 512 KiB blocks in a per-video file under the OS cache
//! directory. RAM holds only coverage metadata (which block indexes are present), never
//! the payload bytes. A block is written whole and chunk-aligned — block `i` always
//! occupies file bytes `[i*BLOCK_SIZE, (i+1)*BLOCK_SIZE)` — so it is either fully
//! present or absent, with no partial-range bookkeeping.
//!
//! Concurrent range requests for the same video cooperate: the first request downloads
//! a missing block from Telegram, writes it to disk, and notifies the others, which then
//! read it from the file instead of re-downloading. A failed download is recorded so all
//! waiters fail fast instead of retrying in a loop.
//!
//! Waiter coordination uses a monotonic [`watch`] version counter rather than a raw
//! [`Notify`](tokio::sync::Notify): the current value is always readable, so a waiter
//! can re-check the cache state after awaiting and can never miss a completion.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{watch, Mutex};

/// Chunk size for both the Telegram download iterator and the on-disk block size.
pub const BLOCK_SIZE: u64 = 512 * 1024;

/// Byte extent of block `i`: `[i*BLOCK_SIZE, min((i+1)*BLOCK_SIZE, total_size)-1]`.
pub fn block_extent(i: u64, total_size: u64) -> (u64, u64) {
    let start = i * BLOCK_SIZE;
    let end = ((i + 1) * BLOCK_SIZE - 1).min(total_size.saturating_sub(1));
    (start, end)
}

/// All block indexes between two byte offsets (inclusive).
pub fn blocks_between(start: u64, end: u64) -> std::ops::RangeInclusive<u64> {
    start / BLOCK_SIZE..=end / BLOCK_SIZE
}

/// A handle to the on-disk cache for a single video. Cheap to clone; all state is shared
/// behind `Arc`s / a `watch` sender.
#[derive(Clone)]
pub struct BlockCache {
    /// Path of the cache file on disk.
    path: PathBuf,
    /// Total size of the video in bytes.
    total_size: u64,
    /// Present blocks (indexes only; file offset is always `i * BLOCK_SIZE`).
    blocks: Arc<Mutex<HashSet<u64>>>,
    /// Blocks currently being downloaded, to deduplicate concurrent fills.
    in_flight: Arc<Mutex<HashSet<u64>>>,
    /// Blocks that failed to download; keeps waiters from retrying in a loop.
    failed: Arc<Mutex<HashMap<u64, String>>>,
    /// Monotonic version bumped on every state change (block stored or failed). Waiters
    /// subscribe and await `changed()`; the current value always reflects the latest
    /// state, so no completion can be missed.
    version: watch::Sender<u64>,
}

impl BlockCache {
    /// Create a cache for a video, rooted at `cache_dir`. The backing file is created
    /// lazily on the first write; `cache_dir` may be shared by many videos.
    pub fn new(cache_dir: &Path, chat_id: i64, msg_id: i32, total_size: u64) -> Self {
        Self {
            path: cache_dir.join(format!("{chat_id}_{msg_id}.bin")),
            total_size,
            blocks: Arc::new(Mutex::new(HashSet::new())),
            in_flight: Arc::new(Mutex::new(HashSet::new())),
            failed: Arc::new(Mutex::new(HashMap::new())),
            version: watch::channel(0).0,
        }
    }

    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    /// Number of bytes cached on disk (sum of present block extents).
    pub async fn cached_bytes(&self) -> u64 {
        let present = self.blocks.lock().await;
        present
            .iter()
            .map(|&i| {
                let (s, e) = block_extent(i, self.total_size);
                e - s + 1
            })
            .sum()
    }

    /// Ensure block `block` is present in the cache, downloading it via `download` if
    /// needed. Concurrent requests for the same block share a single download: the first
    /// claims the in-flight slot; the rest subscribe to the version counter, wait for the
    /// owner to finish, and then read the block from disk. A download failure is recorded
    /// so every waiter receives the error.
    ///
    /// `download` is lazy (an async block) and only polled by the request that owns the
    /// in-flight claim.
    pub async fn ensure<F>(&self, block: u64, download: F) -> Result<(), anyhow::Error>
    where
        F: std::future::Future<Output = Result<Vec<u8>, anyhow::Error>>,
    {
        loop {
            // Fast paths: block already present, or it previously failed.
            if self.blocks.lock().await.contains(&block) {
                return Ok(());
            }
            if let Some(msg) = self.failed.lock().await.get(&block).cloned() {
                return Err(anyhow::anyhow!("block {block} previously failed: {msg}"));
            }

            // Claim the download slot, or wait for whoever owns it.
            let claimed = {
                let mut inflight = self.in_flight.lock().await;
                if inflight.contains(&block) {
                    false
                } else {
                    inflight.insert(block);
                    true
                }
            };

            if !claimed {
                // Wait for the owner to finish. Subscribe to the latest version and
                // re-check the state afterwards — the watch value is always current, so
                // we cannot miss the owner's completion.
                let mut rx = self.version.subscribe();
                if self.blocks.lock().await.contains(&block)
                    || self.failed.lock().await.contains_key(&block)
                {
                    continue; // changed while subscribing; re-evaluate
                }
                let _ = rx.changed().await;
                continue;
            }

            // We own the slot. Defensive re-check (another task cannot own it, but the
            // block may have been stored between claim and here).
            if self.blocks.lock().await.contains(&block) {
                self.in_flight.lock().await.remove(&block);
                return Ok(());
            }
            if let Some(msg) = self.failed.lock().await.get(&block).cloned() {
                self.in_flight.lock().await.remove(&block);
                return Err(anyhow::anyhow!("block {block} previously failed: {msg}"));
            }

            let result = match download.await {
                Ok(bytes) => match write_block_at(&self.path, block, &bytes).await {
                    Ok(()) => Ok(()),
                    Err(e) => Err(anyhow::anyhow!(
                        "failed to write {}: {e}",
                        self.path.display()
                    )),
                },
                Err(e) => Err(e),
            };

            match &result {
                Ok(()) => {
                    self.blocks.lock().await.insert(block);
                    self.failed.lock().await.remove(&block);
                }
                Err(e) => {
                    self.failed.lock().await.insert(block, e.to_string());
                }
            }
            self.in_flight.lock().await.remove(&block);
            // Bump the version separately from send_replace: `*borrow()` holds the
            // watch's internal read lock for the whole statement, and send_replace takes
            // the write lock on the same rwlock — read guard + write lock on the same
            // lock deadlocks on write-preferring platforms (macOS). Copy the value out
            // first so the read guard is released before the write.
            let next = *self.version.borrow() + 1;
            let _ = self.version.send_replace(next);

            return result;
        }
    }

    /// Read a present block as `Bytes`, or `None` if it is not in the cache.
    pub async fn read_block(&self, block: u64) -> std::io::Result<Option<bytes::Bytes>> {
        if !self.blocks.lock().await.contains(&block) {
            return Ok(None);
        }
        let (s, e) = block_extent(block, self.total_size);
        let len = (e - s + 1) as usize;
        let mut file = tokio::fs::File::open(&self.path).await?;
        file.seek(std::io::SeekFrom::Start(s)).await?;
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf).await?;
        Ok(Some(bytes::Bytes::from(buf)))
    }
}

/// Write a whole block at `block * BLOCK_SIZE`, creating the file and its parent
/// directory if needed. Writes are strictly chunk-aligned and non-overlapping.
async fn write_block_at(path: &Path, block: u64, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .await?;
    file.seek(std::io::SeekFrom::Start(block * BLOCK_SIZE)).await?;
    file.write_all(bytes).await?;
    file.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn temp_cache_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("min-mpv-cache-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[tokio::test]
    async fn block_extent_matches_chunk_bounds() {
        assert_eq!(block_extent(0, 1_000_000), (0, 512 * 1024 - 1));
        assert_eq!(
            block_extent(1, 1_000_000),
            (512 * 1024, 1_000_000 - 1),
            "final partial block"
        );
    }

    #[tokio::test]
    async fn blocks_between_covers_range() {
        let r = blocks_between(0, 100);
        assert_eq!((*r.start(), *r.end()), (0, 0));
        let r = blocks_between(512 * 1024 - 1, 512 * 1024);
        assert_eq!((*r.start(), *r.end()), (0, 1));
        let r = blocks_between(1_000, 1_999_999);
        assert_eq!((*r.start(), *r.end()), (0, 3));
    }

    #[tokio::test]
    async fn ensure_writes_and_reads_block() {
        let dir = temp_cache_dir();
        let cache = BlockCache::new(&dir, 1, 2, 512 * 1024);
        let data = vec![7u8; 512 * 1024];
        cache.ensure(0, async { Ok(data) }).await.unwrap();
        let bytes = cache.read_block(0).await.unwrap().unwrap();
        assert_eq!(bytes.len(), 512 * 1024);
        assert!(bytes.iter().all(|b| *b == 7));
        assert_eq!(cache.cached_bytes().await, 512 * 1024);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two concurrent `ensure` calls for the same block must download it exactly once;
    /// the waiter reads the owner's block from disk without downloading itself.
    #[tokio::test]
    async fn ensure_deduplicates_concurrent_downloads() {
        let dir = temp_cache_dir();
        // Total size large enough that block 7 is a full 512 KiB block.
        let cache = BlockCache::new(&dir, 1, 2, 8 * 512 * 1024);
        let download_count = Arc::new(AtomicUsize::new(0));

        // Owner: claims the block, parks mid-download until the test releases it, so the
        // waiter deterministically observes the in-flight claim.
        let owner_cache = cache.clone();
        let owner_count = download_count.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let owner = tokio::spawn(async move {
            owner_cache
                .ensure(7, async move {
                    owner_count.fetch_add(1, Ordering::SeqCst);
                    let _ = started_tx.send(());
                    release_rx.await.unwrap();
                    Ok(vec![9u8; 512 * 1024])
                })
                .await
        });
        started_rx.await.unwrap(); // owner has claimed and started the download

        // Waiter: must never run its download closure.
        let waiter_cache = cache.clone();
        let waiter_count = download_count.clone();
        let waiter = tokio::spawn(async move {
            waiter_cache
                .ensure(7, async move {
                    waiter_count.fetch_add(1, Ordering::SeqCst);
                    panic!("waiter must not download");
                })
                .await
        });

        // Let the waiter observe the in-flight claim, then release the owner.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            download_count.load(Ordering::SeqCst),
            1,
            "only the owner downloads"
        );
        release_tx.send(()).unwrap();
        owner.await.unwrap().unwrap();
        waiter.await.unwrap().unwrap();
        assert_eq!(download_count.load(Ordering::SeqCst), 1);

        assert!(cache.read_block(7).await.unwrap().is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn failed_block_propagates_to_waiters() {
        let dir = temp_cache_dir();
        let cache = BlockCache::new(&dir, 1, 2, 512 * 1024);
        let err = cache.ensure(3, async { Err(anyhow::anyhow!("boom")) }).await;
        assert!(err.is_err());
        let err2 = cache.ensure(3, async { panic!("waiter must not retry") }).await;
        assert!(err2.is_err());
        assert!(err2.unwrap_err().to_string().contains("boom"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
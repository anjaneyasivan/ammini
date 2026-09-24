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
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::watch;

/// Chunk size for both the Telegram download iterator and the on-disk block size.
pub const BLOCK_SIZE: u64 = 512 * 1024;

/// Default quota for the on-disk cache shared by all videos.
pub const CACHE_MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Default max age for cached video files.
pub const CACHE_MAX_AGE: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Magic bytes identifying a cache manifest file (see `BlockCache::save_manifest`).
const MANIFEST_MAGIC: &[u8; 4] = b"MMCM";
const MANIFEST_VERSION: u32 = 1;
/// Binary header: magic(4) + version(4) + total_size(8) + block_count(8).
const MANIFEST_HEADER_LEN: usize = 24;

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

/// Map cached byte ranges to time spans (seconds) assuming a uniform bitrate — the
/// standard approximation, since the block cache is byte-addressed and has no
/// byte↔time index. Spans are clamped to `[0, duration]`; returns empty when
/// `duration <= 0` or `total_bytes == 0`.
pub fn byte_ranges_to_time_spans(
    ranges: &[(u64, u64)],
    total_bytes: u64,
    duration: f64,
) -> Vec<(f64, f64)> {
    if duration <= 0.0 || total_bytes == 0 {
        return Vec::new();
    }
    let to_time = |byte: u64| (byte as f64 / total_bytes as f64 * duration).clamp(0.0, duration);
    ranges
        .iter()
        .map(|&(start, end)| (to_time(start), to_time(end)))
        .filter(|(start, end)| end > start && *start < duration)
        .collect()
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
    // Metadata locks are scoped to single statements (never held across an `.await`),
    // so plain std mutexes are safe and avoid `blocking_lock` panics inside the runtime.
    blocks: Arc<Mutex<HashSet<u64>>>,
    /// Blocks currently being downloaded, to deduplicate concurrent fills.
    in_flight: Arc<Mutex<HashSet<u64>>>,
    /// Blocks that failed to download; keeps waiters from retrying in a loop.
    failed: Arc<Mutex<HashMap<u64, String>>>,
    /// Monotonic version bumped on every state change (block stored or failed). Waiters
    /// subscribe and await `changed()`; the current value always reflects the latest
    /// state, so no completion can be missed.
    version: watch::Sender<u64>,
    /// Serializes manifest rewrites so concurrent block stores never interleave writes.
    meta_lock: Arc<Mutex<()>>,
}

impl BlockCache {
    /// Create a cache for a video, rooted at `cache_dir`. The backing file is created
    /// lazily on the first write; `cache_dir` may be shared by many videos. Coverage is
    /// restored from a sidecar manifest written by a previous session, so blocks already
    /// on disk are not re-downloaded.
    pub fn new(cache_dir: &Path, chat_id: i64, msg_id: i32, total_size: u64) -> Self {
        let cache = Self {
            path: cache_dir.join(format!("{chat_id}_{msg_id}.bin")),
            total_size,
            blocks: Arc::new(Mutex::new(HashSet::new())),
            in_flight: Arc::new(Mutex::new(HashSet::new())),
            failed: Arc::new(Mutex::new(HashMap::new())),
            version: watch::channel(0).0,
            meta_lock: Arc::new(std::sync::Mutex::new(())),
        };
        cache.load_manifest();
        cache
    }

    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    /// Number of bytes cached on disk (sum of present block extents).
    pub async fn cached_bytes(&self) -> u64 {
        let present = self.blocks.lock().unwrap();
        present
            .iter()
            .map(|&i| {
                let (s, e) = block_extent(i, self.total_size);
                e - s + 1
            })
            .sum()
    }

    /// Contiguous byte ranges `[start, end)` covering every present block, with
    /// adjacent blocks merged. Blocks are chunk-aligned, so ranges align to
    /// `BLOCK_SIZE` except for a final partial block. Used to render cached coverage
    /// on the seekbar; the lock is scoped to this call.
    pub fn cached_byte_ranges(&self) -> Vec<(u64, u64)> {
        let present = self.blocks.lock().unwrap();
        let mut indexes: Vec<u64> = present.iter().copied().collect();
        indexes.sort_unstable();
        let mut ranges: Vec<(u64, u64)> = Vec::new();
        for &i in &indexes {
            let (start, end) = block_extent(i, self.total_size);
            let end = end + 1; // convert inclusive extent to exclusive end
            if let Some((_, last_end)) = ranges.last_mut()
                && *last_end == start
            {
                *last_end = end;
            } else {
                ranges.push((start, end));
            }
        }
        ranges
    }

    fn manifest_path(&self) -> PathBuf {
        PathBuf::from(format!("{}.meta", self.path.display()))
    }

    /// Restore block coverage from the sidecar manifest, if any. Any mismatch (bad
    /// magic/version, different total size, truncated bitmap, missing `.bin`) is treated
    /// as "no coverage": the cache simply refills from the network.
    fn load_manifest(&self) {
        if !self.path.exists() {
            return;
        }
        let Ok(bytes) = std::fs::read(self.manifest_path()) else {
            return;
        };
        let block_count = block_count_for(self.total_size) as usize;
        if bytes.len() != MANIFEST_HEADER_LEN + block_count.div_ceil(8)
            || &bytes[0..4] != MANIFEST_MAGIC
            || u32::from_le_bytes(bytes[4..8].try_into().unwrap()) != MANIFEST_VERSION
            || u64::from_le_bytes(bytes[8..16].try_into().unwrap()) != self.total_size
            || u64::from_le_bytes(bytes[16..24].try_into().unwrap()) as usize != block_count
        {
            return;
        }
        let mut present = HashSet::new();
        for (i, byte) in bytes[MANIFEST_HEADER_LEN..].iter().enumerate() {
            for bit in 0..8 {
                if byte & (1 << bit) != 0 {
                    present.insert((i * 8 + bit) as u64);
                }
            }
        }
        let restored = present.len();
        self.blocks.lock().unwrap().extend(present);
        tracing::debug!(
            "cache: restored {restored} blocks from {}",
            self.manifest_path().display()
        );
    }

    /// Atomically rewrite the manifest (tmp + rename) reflecting the current `blocks`
    /// set. Serialized by `meta_lock` so concurrent block stores never interleave
    /// writes; a failed write only costs reuse, never correctness.
    fn save_manifest(&self) {
        let _guard = self.meta_lock.lock().unwrap_or_else(|e| e.into_inner());
        let block_count = block_count_for(self.total_size);
        let mut bytes = Vec::with_capacity(MANIFEST_HEADER_LEN + block_count.div_ceil(8) as usize);
        bytes.extend_from_slice(MANIFEST_MAGIC);
        bytes.extend_from_slice(&MANIFEST_VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.total_size.to_le_bytes());
        bytes.extend_from_slice(&block_count.to_le_bytes());
        {
            let present = self.blocks.lock().unwrap();
            let mut bitmap = vec![0u8; block_count.div_ceil(8) as usize];
            for &i in present.iter() {
                if i < block_count {
                    bitmap[i as usize / 8] |= 1 << (i % 8);
                }
            }
            bytes.extend_from_slice(&bitmap);
        }
        let target = self.manifest_path();
        let tmp = PathBuf::from(format!("{}.tmp", target.display()));
        match std::fs::write(&tmp, &bytes).and_then(|_| std::fs::rename(&tmp, &target)) {
            Ok(()) => {}
            Err(e) => tracing::debug!("cache: manifest write failed for {}: {e}", target.display()),
        }
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
            if self.blocks.lock().unwrap().contains(&block) {
                return Ok(());
            }
            if let Some(msg) = self.failed.lock().unwrap().get(&block).cloned() {
                return Err(anyhow::anyhow!("block {block} previously failed: {msg}"));
            }

            // Claim the download slot, or wait for whoever owns it.
            let claimed = {
                let mut inflight = self.in_flight.lock().unwrap();
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
                if self.blocks.lock().unwrap().contains(&block)
                    || self.failed.lock().unwrap().contains_key(&block)
                {
                    continue; // changed while subscribing; re-evaluate
                }
                let _ = rx.changed().await;
                continue;
            }

            // We own the slot. Defensive re-check (another task cannot own it, but the
            // block may have been stored between claim and here).
            if self.blocks.lock().unwrap().contains(&block) {
                self.in_flight.lock().unwrap().remove(&block);
                return Ok(());
            }
            if let Some(msg) = self.failed.lock().unwrap().get(&block).cloned() {
                self.in_flight.lock().unwrap().remove(&block);
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
                    self.blocks.lock().unwrap().insert(block);
                    self.failed.lock().unwrap().remove(&block);
                    self.save_manifest();
                }
                Err(e) => {
                    self.failed.lock().unwrap().insert(block, e.to_string());
                }
            }
            self.in_flight.lock().unwrap().remove(&block);
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
        if !self.blocks.lock().unwrap().contains(&block) {
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

/// Number of blocks a video of `total_size` bytes is split into.
fn block_count_for(total_size: u64) -> u64 {
    total_size.div_ceil(BLOCK_SIZE)
}

/// Delete a video file together with its manifest (and any stray tmp file).
fn remove_file_pair(path: &Path) {
    let _ = std::fs::remove_file(path);
    let meta = PathBuf::from(format!("{}.meta", path.display()));
    let _ = std::fs::remove_file(&meta);
    let tmp = PathBuf::from(format!("{}.tmp", meta.display()));
    let _ = std::fs::remove_file(tmp);
}

/// Garbage-collect the cache directory: drop `.bin` files (with their manifests) older
/// than `max_age`, then delete oldest-by-mtime first until the remaining bytes fit
/// under `max_bytes`. Orphaned `.meta`/`.tmp` files are removed. Returns the number of
/// bytes deleted. Runs once at startup, before any new files can appear; a missing
/// directory is not an error.
pub fn sweep_cache_dir(dir: &Path, max_bytes: u64, max_age: Duration) -> std::io::Result<u64> {
    let mut entries: Vec<(PathBuf, SystemTime, u64)> = Vec::new();
    let mut orphaned_meta: Vec<PathBuf> = Vec::new();
    match std::fs::read_dir(dir) {
        Ok(iter) => {
            for entry in iter.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "meta") {
                    if !path.with_extension("bin").exists() {
                        orphaned_meta.push(path);
                    }
                    continue;
                }
                if path.extension().is_some_and(|e| e == "tmp") {
                    let _ = std::fs::remove_file(&path);
                    continue;
                }
                if path.extension().is_some_and(|e| e == "bin")
                    && let Ok(meta) = entry.metadata()
                    && let Ok(mtime) = meta.modified()
                {
                    entries.push((path, mtime, meta.len()));
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e),
    }

    let now = SystemTime::now();
    let mut removed_bytes = 0u64;
    entries.retain(|(path, mtime, len)| {
        let stale = now.duration_since(*mtime).is_ok_and(|age| age > max_age);
        if stale {
            remove_file_pair(path);
            removed_bytes += len;
        }
        !stale
    });

    let total: u64 = entries.iter().map(|(_, _, len)| *len).sum();
    if total > max_bytes {
        // Oldest first; `sort_by_key` is stable, so equal mtimes keep dir order.
        entries.sort_by_key(|(_, mtime, _)| *mtime);
        let mut remaining = total;
        for (path, _, len) in entries {
            remove_file_pair(&path);
            remaining = remaining.saturating_sub(len);
            removed_bytes += len;
            if remaining <= max_bytes {
                break;
            }
        }
    }

    for meta in orphaned_meta {
        let _ = std::fs::remove_file(meta);
    }
    Ok(removed_bytes)
}

/// Write a whole block at `block * BLOCK_SIZE`, creating the file and its parent
/// directory if needed. Writes are strictly chunk-aligned and non-overlapping.
async fn write_block_at(path: &Path, block: u64, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    // Never truncate: blocks are written at their own offsets and the file may already
    // hold other blocks (or a manifest-driven partial reuse).
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)
        .await?;
    file.seek(std::io::SeekFrom::Start(block * BLOCK_SIZE))
        .await?;
    file.write_all(bytes).await?;
    file.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::time::Duration;

    /// Unique per call: tests run in parallel and would otherwise wipe each other's
    /// directories (the fix for an intermittent NotFound in longer tests).
    static DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_cache_dir() -> PathBuf {
        let n = DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("ammini-cache-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
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

    /// A cache with only given blocks marked present; testing coverage queries
    /// without touching the network.
    fn cache_with_blocks(total_size: u64, present: &[u64]) -> BlockCache {
        let cache = BlockCache::new(&temp_cache_dir(), 1, 1, total_size);
        let mut blocks = cache.blocks.lock().unwrap();
        blocks.extend(present.iter().copied());
        drop(blocks);
        cache
    }

    #[tokio::test]
    async fn cached_byte_ranges_empty_when_nothing_cached() {
        let cache = cache_with_blocks(1_000_000, &[]);
        assert!(cache.cached_byte_ranges().is_empty());
    }

    #[tokio::test]
    async fn cached_byte_ranges_returned_sorted_even_for_unsorted_presence() {
        // 3 MB = 6 blocks; indexes 3, 0, 1 are all valid.
        let cache = cache_with_blocks(6 * BLOCK_SIZE, &[3, 0, 1]);
        assert_eq!(
            cache.cached_byte_ranges(),
            vec![(0, 2 * BLOCK_SIZE), (3 * BLOCK_SIZE, 4 * BLOCK_SIZE)]
        );
    }

    #[tokio::test]
    async fn cached_byte_ranges_merge_adjacent_and_keep_gaps() {
        // 4 MB = 8 blocks; indexes 2, 3, 5 are all valid.
        let cache = cache_with_blocks(8 * BLOCK_SIZE, &[2, 3, 5]);
        assert_eq!(
            cache.cached_byte_ranges(),
            vec![
                (2 * BLOCK_SIZE, 4 * BLOCK_SIZE),
                (5 * BLOCK_SIZE, 6 * BLOCK_SIZE)
            ]
        );
    }

    #[tokio::test]
    async fn cached_byte_ranges_partial_final_block() {
        let cache = cache_with_blocks(600_000, &[0, 1]); // block 1 is 75 KiB
        assert_eq!(cache.cached_byte_ranges(), vec![(0, 600_000)]);
    }

    #[test]
    fn byte_ranges_to_time_spans_maps_linearly() {
        let spans = byte_ranges_to_time_spans(&[(0, 500_000)], 1_000_000, 100.0);
        assert_eq!(spans, vec![(0.0, 50.0)]);
    }

    #[test]
    fn byte_ranges_to_time_spans_clamps_to_duration() {
        assert_eq!(
            byte_ranges_to_time_spans(&[(900_000, 1_200_000)], 1_000_000, 100.0),
            vec![(90.0, 100.0)]
        );
        // A range entirely past the end of playback collapses and is dropped.
        assert!(byte_ranges_to_time_spans(&[(1_500_000, 1_800_000)], 1_000_000, 100.0).is_empty());
    }

    #[test]
    fn byte_ranges_to_time_spans_rejects_no_media() {
        assert!(byte_ranges_to_time_spans(&[(0, 500_000)], 1_000_000, 0.0).is_empty());
        assert!(byte_ranges_to_time_spans(&[(0, 500_000)], 0, 100.0).is_empty());
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
        let err = cache
            .ensure(3, async { Err(anyhow::anyhow!("boom")) })
            .await;
        assert!(err.is_err());
        let err2 = cache
            .ensure(3, async { panic!("waiter must not retry") })
            .await;
        assert!(err2.is_err());
        assert!(err2.unwrap_err().to_string().contains("boom"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A reopened cache (new session, same video ids) must restore block coverage from
    /// the manifest and serve stored blocks from disk without re-downloading.
    #[tokio::test]
    async fn manifest_restores_blocks_on_reopen() {
        let dir = temp_cache_dir();
        let total = 8 * BLOCK_SIZE;
        let cache = BlockCache::new(&dir, 1, 2, total);
        cache
            .ensure(3, async { Ok(vec![9u8; BLOCK_SIZE as usize]) })
            .await
            .unwrap();
        cache
            .ensure(7, async { Ok(vec![1u8; BLOCK_SIZE as usize]) })
            .await
            .unwrap();

        let reopened = BlockCache::new(&dir, 1, 2, total);
        assert!(reopened.read_block(3).await.unwrap().is_some());
        assert!(reopened.read_block(7).await.unwrap().is_some());
        // Present blocks must never invoke the download closure.
        reopened
            .ensure(3, async { panic!("must not re-download") })
            .await
            .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A manifest recording a different total size refers to different content and must
    /// be ignored, so the block is fetched again.
    #[tokio::test]
    async fn manifest_ignored_when_size_mismatches() {
        let dir = temp_cache_dir();
        let cache = BlockCache::new(&dir, 1, 2, 8 * BLOCK_SIZE);
        cache
            .ensure(0, async { Ok(vec![7u8; BLOCK_SIZE as usize]) })
            .await
            .unwrap();

        let reopened = BlockCache::new(&dir, 1, 2, 16 * BLOCK_SIZE);
        let mut downloaded = false;
        reopened
            .ensure(0, async {
                downloaded = true;
                Ok(vec![7u8; BLOCK_SIZE as usize])
            })
            .await
            .unwrap();
        assert!(downloaded, "mismatched manifest must not prevent download");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sweep_removes_stale_and_oversized_files() {
        let dir = temp_cache_dir();

        let stale = dir.join("1_1.bin");
        std::fs::write(&stale, vec![0u8; 1000]).unwrap();
        let stale_time = SystemTime::now() - Duration::from_secs(40 * 24 * 3600);
        std::fs::File::options()
            .write(true)
            .open(&stale)
            .unwrap()
            .set_modified(stale_time)
            .unwrap();

        let fresh_a = dir.join("2_2.bin");
        let fresh_b = dir.join("3_3.bin");
        std::fs::write(&fresh_a, vec![0u8; 2000]).unwrap();
        std::fs::write(&fresh_b, vec![0u8; 3000]).unwrap();
        std::fs::write(dir.join("2_2.bin.meta"), b"junk").unwrap();
        std::fs::write(dir.join("4_4.bin.tmp"), b"x").unwrap();

        let removed = sweep_cache_dir(&dir, 2500, Duration::from_secs(30 * 24 * 3600)).unwrap();

        assert!(!stale.exists(), "stale file must be deleted");
        // Budget after removing `stale` is 5000 bytes, cap 2500: both fresh files go too.
        assert!(!fresh_a.exists());
        assert!(!fresh_b.exists());
        assert!(
            !dir.join("2_2.bin.meta").exists(),
            "manifest must go with its video"
        );
        assert!(
            !dir.join("4_4.bin.tmp").exists(),
            "stray tmp file must be removed"
        );
        assert_eq!(removed, 6000);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sweep_missing_dir_is_not_an_error() {
        let dir = temp_cache_dir();
        assert_eq!(
            sweep_cache_dir(&dir, 100, Duration::from_secs(60)).unwrap(),
            0
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

use anyhow::{Context, Result};
use rangemap::RangeSet;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{watch, Mutex};

/// Cache entry for a single Telegram document.
pub struct DocCache {
    file: File,
    total_size: u64,
    downloaded: Mutex<RangeSet<u64>>,
    fetch_watermark: watch::Sender<u64>,
    path: PathBuf,
}

impl DocCache {
    /// Create a new cache entry for a document.
    pub async fn create(
        cache_dir: &PathBuf,
        chat_id: i64,
        msg_id: i32,
        total_size: u64,
    ) -> Result<Arc<Self>> {
        let path = cache_dir.join(format!("{}_{}.bin", chat_id, msg_id));

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .await
            .context("Failed to create cache file")?;

        file.set_len(total_size).await.ok();

        let (sender, _receiver) = watch::channel(0u64);

        let cache = Arc::new(Self {
            file,
            total_size,
            downloaded: Mutex::new(RangeSet::new()),
            fetch_watermark: sender,
            path,
        });

        Ok(cache)
    }

    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    /// Check if a range is fully downloaded.
    /// RangeSet::contains takes a single value, so we check if both start and end are covered.
    pub async fn is_range_downloaded(&self, start: u64, end: u64) -> bool {
        let downloaded = self.downloaded.lock().await;
        downloaded.contains(&start) && downloaded.contains(&(end - 1))
    }

    pub fn watermark(&self) -> u64 {
        *self.fetch_watermark.borrow()
    }

    pub async fn wait_for_watermark(&self, target: u64) -> Result<()> {
        let mut receiver = self.fetch_watermark.subscribe();
        loop {
            if *receiver.borrow_and_update() >= target {
                return Ok(());
            }
            receiver
                .changed()
                .await
                .context("Watermark channel closed")?;
        }
    }

    pub async fn write_at(&self, offset: u64, data: &[u8]) -> Result<()> {
        let mut file = self.file.try_clone().await?;
        file.seek(std::io::SeekFrom::Start(offset)).await?;
        file.write_all(data).await?;

        let mut downloaded = self.downloaded.lock().await;
        downloaded.insert(offset..offset + data.len() as u64);

        let new_end = offset + data.len() as u64;
        if new_end > *self.fetch_watermark.borrow() {
            self.fetch_watermark.send_replace(new_end);
        }

        Ok(())
    }

    pub async fn read_at(&self, offset: u64, len: u64) -> Result<Vec<u8>> {
        let mut file = self.file.try_clone().await?;
        file.seek(std::io::SeekFrom::Start(offset)).await?;

        let mut buf = vec![0u8; len as usize];
        tokio::io::AsyncReadExt::read_exact(&mut file, &mut buf).await?;
        Ok(buf)
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

/// Manages all document caches.
pub struct DocCacheManager {
    cache_dir: PathBuf,
    caches: HashMap<(i64, i32), Arc<DocCache>>,
}

impl DocCacheManager {
    pub fn new(cache_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&cache_dir).ok();
        Self {
            cache_dir,
            caches: HashMap::new(),
        }
    }

    pub async fn get_or_create(
        &mut self,
        chat_id: i64,
        msg_id: i32,
        total_size: u64,
    ) -> Result<Arc<DocCache>> {
        let key = (chat_id, msg_id);

        if let Some(cache) = self.caches.get(&key) {
            return Ok(cache.clone());
        }

        let cache = DocCache::create(&self.cache_dir, chat_id, msg_id, total_size).await?;
        self.caches.insert(key, cache.clone());
        Ok(cache)
    }

    pub fn remove(&mut self, chat_id: i64, msg_id: i32) {
        let key = (chat_id, msg_id);
        if let Some(cache) = self.caches.remove(&key) {
            std::fs::remove_file(cache.path()).ok();
        }
    }

    pub fn clear(&mut self) {
        for (_, cache) in self.caches.drain() {
            std::fs::remove_file(cache.path()).ok();
        }
    }
}

// Import AsyncReadExt for DocCache::read_at.
use tokio::io::AsyncReadExt;

// Auto-patched by Alloma
// Timestamp: 2025-06-02 00:35:19
// User: kartik4091

#![allow(warnings)]

use std::collections::{HashMap, VecDeque};
use tokio::sync::RwLock;
use std::sync::Arc;
use std::time::Duration;
use crate::core::error::PdfError;

pub struct CacheManager {
    memory_cache: Arc<RwLock<MemoryCache>>,
    disk_cache: Arc<RwLock<DiskCache>>,
    config: CacheConfig,
    stats: Arc<RwLock<CacheStats>>,
}

#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub memory_size: usize,
    pub disk_size: usize,
    pub cache_ttl: Duration,
    pub eviction_policy: EvictionPolicy,
    pub compression_enabled: bool,
}

#[derive(Debug, Clone)]
pub enum EvictionPolicy {
    LRU,
    LFU,
    FIFO,
    Custom(Box<dyn EvictionStrategy>),
}

#[async_trait::async_trait]
pub trait EvictionStrategy: Send + Sync {
    async fn select_victim(&self, cache: &HashMap<String, CacheEntry>) -> Option<String>;
    async fn update_stats(&mut self, key: &str, operation: CacheOperation);
}

#[derive(Debug, Clone)]
pub enum CacheOperation {
    Read,
    Write,
    Delete,
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
    pub evictions: usize,
    pub memory_usage: usize,
    pub disk_usage: usize,
    pub operation_times: HashMap<String, Duration>,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    key: String,
    data: Vec<u8>,
    created_at: chrono::DateTime<chrono::Utc>,
    last_accessed: chrono::DateTime<chrono::Utc>,
    access_count: usize,
    size: usize,
}

struct MemoryCache {
    entries: HashMap<String, CacheEntry>,
    size: usize,
    max_size: usize,
    eviction_queue: VecDeque<String>,
}

struct DiskCache {
    base_path: std::path::PathBuf,
    entries: HashMap<String, CacheEntry>,
    size: usize,
    max_size: usize,
}

impl CacheManager {
    pub fn new(config: CacheConfig) -> Self {
        CacheManager {
            memory_cache: Arc::new(RwLock::new(MemoryCache::new(config.memory_size))),
            disk_cache: Arc::new(RwLock::new(DiskCache::new(config.disk_size))),
            config,
            stats: Arc::new(RwLock::new(CacheStats {
                hits: 0,
                misses: 0,
                evictions: 0,
                memory_usage: 0,
                disk_usage: 0,
                operation_times: HashMap::new(),
            })),
        }
    }

    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, PdfError> {
        let start_time = std::time::Instant::now();

        // Try memory cache first
        if let Some(data) = self.get_from_memory(key).await? {
            self.update_stats(true, start_time.elapsed()).await;
            return Ok(Some(data));
        }

        // Try disk cache
        if let Some(data) = self.get_from_disk(key).await? {
            // Move to memory cache if possible
            self.promote_to_memory(key, &data).await?;
            self.update_stats(true, start_time.elapsed()).await;
            return Ok(Some(data));
        }

        self.update_stats(false, start_time.elapsed()).await;
        Ok(None)
    }

    pub async fn put(&self, key: String, data: Vec<u8>) -> Result<(), PdfError> {
        let start_time = std::time::Instant::now();
        let size = data.len();

        // Try memory cache first
        if size <= self.config.memory_size {
            self.put_in_memory(key.clone(), data.clone()).await?;
        } else {
            // Store in disk cache if too large for memory
            self.put_in_disk(key, data).await?;
        }

        self.update_stats(true, start_time.elapsed()).await;
        Ok(())
    }

    pub async fn remove(&self, key: &str) -> Result<(), PdfError> {
        let start_time = std::time::Instant::now();

        // Remove from both caches
        self.remove_from_memory(key).await?;
        self.remove_from_disk(key).await?;

        self.update_stats(true, start_time.elapsed()).await;
        Ok(())
    }

    pub async fn clear(&self) -> Result<(), PdfError> {
        let start_time = std::time::Instant::now();

        // Clear both caches
        self.memory_cache.write().await.clear();
        self.disk_cache.write().await.clear();

        self.update_stats(true, start_time.elapsed()).await;
        Ok(())
    }

    async fn get_from_memory(&self, key: &str) -> Result<Option<Vec<u8>>, PdfError> {
        let mut cache = self.memory_cache.write().await;
        
        if let Some(entry) = cache.entries.get_mut(key) {
            entry.last_accessed = chrono::Utc::now();
            entry.access_count += 1;
            Ok(Some(entry.data.clone()))
        } else {
            Ok(None)
        }
    }

    async fn get_from_disk(&self, key: &str) -> Result<Option<Vec<u8>>, PdfError> {
        let cache = self.disk_cache.read().await;
        
        if let Some(entry) = cache.entries.get(key) {
            let path = cache.base_path.join(key);
            Ok(Some(tokio::fs::read(path).await?))
        } else {
            Ok(None)
        }
    }

    async fn put_in_memory(&self, key: String, data: Vec<u8>) -> Result<(), PdfError> {
        let mut cache = self.memory_cache.write().await;
        let size = data.len();

        // Evict if necessary
        while cache.size + size > cache.max_size {
            if let Some(evicted_key) = cache.evict_entry().await? {
                let mut stats = self.stats.write().await;
                stats.evictions += 1;
            }
        }

        // Add new entry
        cache.add_entry(CacheEntry {
            key: key.clone(),
            data,
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            access_count: 0,
            size,
        });

        Ok(())
    }

    async fn put_in_disk(&self, key: String, data: Vec<u8>) -> Result<(), PdfError> {
        let mut cache = self.disk_cache.write().await;
        let size = data.len();

        // Evict if necessary
        while cache.size + size > cache.max_size {
            if let Some(evicted_key) = cache.evict_entry().await? {
                let mut stats = self.stats.write().await;
                stats.evictions += 1;
            }
        }

        // Write to disk
        let path = cache.base_path.join(&key);
        tokio::fs::write(&path, &data).await?;

        // Add entry
        cache.entries.insert(key.clone(), CacheEntry {
            key,
            data: Vec::new(), // Don't store data in memory
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            access_count: 0,
            size,
        });

        cache.size += size;
        Ok(())
    }

    async fn promote_to_memory(&self, key: &str, data: &[u8]) -> Result<(), PdfError> {
        if data.len() <= self.config.memory_size {
            self.put_in_memory(key.to_string(), data.to_vec()).await?;
        }
        Ok(())
    }

    async fn remove_from_memory(&self, key: &str) -> Result<(), PdfError> {
        let mut cache = self.memory_cache.write().await;
        if let Some(entry) = cache.entries.remove(key) {
            cache.size -= entry.size;
        }
        Ok(())
    }

    async fn remove_from_disk(&self, key: &str) -> Result<(), PdfError> {
        let mut cache = self.disk_cache.write().await;
        if let Some(entry) = cache.entries.remove(key) {
            let path = cache.base_path.join(key);
            if path.exists() {
                tokio::fs::remove_file(path).await?;
            }
            cache.size -= entry.size;
        }
        Ok(())
    }

    async fn update_stats(&self, hit: bool, duration: Duration) -> Result<(), PdfError> {
        let mut stats = self.stats.write().await;
        if hit {
            stats.hits += 1;
        } else {
            stats.misses += 1;
        }
        
        let operation = if hit { "hit" } else { "miss" };
        stats.operation_times.insert(operation.to_string(), duration);
        Ok(())
    }
}

impl MemoryCache {
    fn new(max_size: usize) -> Self {
        MemoryCache {
            entries: HashMap::new(),
            size: 0,
            max_size,
            eviction_queue: VecDeque::new(),
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.size = 0;
        self.eviction_queue.clear();
    }

    async fn evict_entry(&mut self) -> Result<Option<String>, PdfError> {
        if let Some(key) = self.eviction_queue.pop_front() {
            if let Some(entry) = self.entries.remove(&key) {
                self.size -= entry.size;
                return Ok(Some(key));
            }
        }
        Ok(None)
    }

    fn add_entry(&mut self, entry: CacheEntry) {
        let size = entry.size;
        let key = entry.key.clone();
        self.entries.insert(key.clone(), entry);
        self.size += size;
        self.eviction_queue.push_back(key);
    }
}

impl DiskCache {
    fn new(max_size: usize) -> Self {
        DiskCache {
            base_path: std::path::PathBuf::from("cache"),
            entries: HashMap::new(),
            size: 0,
            max_size,
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.size = 0;
    }

    async fn evict_entry(&mut self) -> Result<Option<String>, PdfError> {
        if let Some((key, entry)) = self.entries.iter().next() {
            let key = key.clone();
            let path = self.base_path.join(&key);
            if path.exists() {
                tokio::fs::remove_file(path).await?;
            }
            self.entries.remove(&key);
            self.size -= entry.size;
            return Ok(Some(key));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_operations() {
        let config = CacheConfig {
            memory_size: 1024 * 1024, // 1MB
            disk_size: 1024 * 1024 * 10, // 10MB
            cache_ttl: Duration::from_secs(3600),
            eviction_policy: EvictionPolicy::LRU,
            compression_enabled: true,
        };

        let cache = CacheManager::new(config);

        // Test put and get
        let key = "test_key".to_string();
        let data = vec![1, 2, 3, 4, 5];
        
        cache.put(key.clone(), data.clone()).await.unwrap();
        let retrieved = cache.get(&key).await.unwrap();
        
        assert_eq!(retrieved, Some(data));
    }
}
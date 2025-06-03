//! Hash computation and verification module
//! Author: kartik4091
//! Created: 2025-06-03 10:27:11 UTC

use std::{
    sync::Arc,
    collections::HashMap,
    time::{Instant, Duration},
    io::{self, Read, BufReader},
};
use tokio::{
    sync::{RwLock, Semaphore},
    fs::File,
    io::{AsyncRead, AsyncReadExt},
};
use sha2::{Sha256, Sha512, Digest};
use md5::{Md5, Md5State};
use blake3::Hasher as Blake3;
use tracing::{info, warn, error, debug, instrument};
use serde::{Serialize, Deserialize};

use crate::{
    error::{Result, ForensicError},
    metrics::MetricsCollector,
    types::VerificationLevel,
};

/// Hash computation state
#[derive(Debug)]
struct HashState {
    /// Active computations
    active_computations: usize,
    /// Computed hashes
    computed_hashes: HashMap<String, DocumentHashes>,
    /// Computation history
    computation_history: Vec<ComputationRecord>,
    /// Start time
    start_time: Instant,
    /// Total bytes processed
    bytes_processed: u64,
}

/// Document hashes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentHashes {
    /// Document ID
    pub document_id: String,
    /// MD5 hash
    pub md5: String,
    /// SHA256 hash
    pub sha256: String,
    /// SHA512 hash
    pub sha512: String,
    /// BLAKE3 hash
    pub blake3: String,
    /// Computation timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Computation record for history tracking
#[derive(Debug)]
struct ComputationRecord {
    /// Document ID
    document_id: String,
    /// Operation type
    operation: HashOperation,
    /// Start time
    start_time: Instant,
    /// Duration
    duration: Duration,
    /// Bytes processed
    bytes_processed: u64,
    /// Success status
    success: bool,
}

/// Hash operation type
#[derive(Debug, Clone, Copy)]
enum HashOperation {
    /// Compute all hashes
    ComputeAll,
    /// Verify existing hashes
    Verify,
    /// Update specific hash
    Update(HashType),
}

/// Hash type
#[derive(Debug, Clone, Copy)]
pub enum HashType {
    /// MD5 hash
    Md5,
    /// SHA256 hash
    Sha256,
    /// SHA512 hash
    Sha512,
    /// BLAKE3 hash
    Blake3,
}

/// Hash computation configuration
#[derive(Debug, Clone)]
pub struct HashConfig {
    /// Buffer size for reading
    pub buffer_size: usize,
    /// Maximum concurrent computations
    pub max_concurrent: usize,
    /// Operation timeout
    pub timeout: Duration,
    /// Cache computed hashes
    pub enable_cache: bool,
}

impl Default for HashConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1024 * 1024, // 1MB
            max_concurrent: num_cpus::get(),
            timeout: Duration::from_secs(300),
            enable_cache: true,
        }
    }
}

/// Hash handler for computing and verifying document hashes
pub struct HashHandler {
    /// Hash state
    state: Arc<RwLock<HashState>>,
    /// Rate limiter
    rate_limiter: Arc<Semaphore>,
    /// Metrics collector
    metrics: Arc<MetricsCollector>,
    /// Configuration
    config: Arc<HashConfig>,
}

impl HashHandler {
    /// Creates a new hash handler
    #[instrument(skip(metrics))]
    pub fn new(config: HashConfig, metrics: Arc<MetricsCollector>) -> Self {
        info!("Initializing HashHandler");
        
        Self {
            state: Arc::new(RwLock::new(HashState {
                active_computations: 0,
                computed_hashes: HashMap::new(),
                computation_history: Vec::new(),
                start_time: Instant::now(),
                bytes_processed: 0,
            })),
            rate_limiter: Arc::new(Semaphore::new(config.max_concurrent)),
            metrics,
            config: Arc::new(config),
        }
    }

    /// Computes all hashes for a document
    #[instrument(skip(self, document), err(Debug))]
    pub async fn compute_hashes(&self, document: &Document) -> Result<DocumentHashes> {
        debug!("Computing hashes for document {}", document.id());
        
        let _permit = self.acquire_permit().await?;
        let start = Instant::now();

        // Update state
        {
            let mut state = self.state.write().await;
            state.active_computations += 1;
        }

        // Track metrics
        self.metrics.increment_counter("hash_computations_started").await;

        // Try cache first if enabled
        if self.config.enable_cache {
            if let Some(cached) = self.get_cached_hashes(document).await {
                return Ok(cached);
            }
        }

        let result = self.compute_all_hashes(document).await;

        // Update state and metrics
        {
            let mut state = self.state.write().await;
            state.active_computations -= 1;
            
            if let Ok(ref hashes) = result {
                state.computed_hashes.insert(document.id().to_string(), hashes.clone());
                state.computation_history.push(ComputationRecord {
                    document_id: document.id().to_string(),
                    operation: HashOperation::ComputeAll,
                    start_time: start,
                    duration: start.elapsed(),
                    bytes_processed: document.size(),
                    success: true,
                });
            }
        }

        // Track metrics
        self.metrics.increment_counter(
            if result.is_ok() { "hash_computations_completed" } else { "hash_computations_failed" }
        ).await;
        self.metrics.observe_duration("hash_computation_duration", start.elapsed()).await;

        result
    }

    /// Gets cached hashes if available
    async fn get_cached_hashes(&self, document: &Document) -> Option<DocumentHashes> {
        let state = self.state.read().await;
        state.computed_hashes.get(&document.id().to_string()).cloned()
    }

    /// Acquires a permit for computation
    async fn acquire_permit(&self) -> Result<SemaphorePermit> {
        match tokio::time::timeout(self.config.timeout, self.rate_limiter.acquire()).await {
            Ok(Ok(permit)) => Ok(permit),
            Ok(Err(e)) => Err(ForensicError::Concurrency(format!("Failed to acquire permit: {}", e))),
            Err(_) => Err(ForensicError::Concurrency("Permit acquisition timeout".to_string())),
        }
    }

    /// Computes all hashes for a document
    async fn compute_all_hashes(&self, document: &Document) -> Result<DocumentHashes> {
        let mut buffer = vec![0u8; self.config.buffer_size];
        let mut file = document.open_async().await?;

        let mut md5 = Md5::new();
        let mut sha256 = Sha256::new();
        let mut sha512 = Sha512::new();
        let mut blake3 = Blake3::new();

        let mut bytes_read = 0u64;

        loop {
            let n = file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }

            md5.update(&buffer[..n]);
            sha256.update(&buffer[..n]);
            sha512.update(&buffer[..n]);
            blake3.update(&buffer[..n]);

            bytes_read += n as u64;
        }

        // Update metrics
        self.metrics.increment_counter_by("bytes_processed", bytes_read).await;

        Ok(DocumentHashes {
            document_id: document.id().to_string(),
            md5: format!("{:x}", md5.finalize()),
            sha256: format!("{:x}", sha256.finalize()),
            sha512: format!("{:x}", sha512.finalize()),
            blake3: format!("{:x}", blake3.finalize()),
            timestamp: chrono::Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;

    async fn create_test_file(content: &[u8]) -> NamedTempFile {
        let file = NamedTempFile::new().unwrap();
        let mut async_file = File::create(file.path()).await.unwrap();
        async_file.write_all(content).await.unwrap();
        file
    }

    #[tokio::test]
    async fn test_hash_computation() {
        let metrics = Arc::new(MetricsCollector::new());
        let config = HashConfig::default();
        let handler = HashHandler::new(config, metrics.clone());

        let test_data = b"Hello, World!";
        let test_file = create_test_file(test_data).await;
        let document = Document::new(test_file.path());

        let result = handler.compute_hashes(&document).await;
        assert!(result.is_ok());

        let hashes = result.unwrap();
        assert!(!hashes.md5.is_empty());
        assert!(!hashes.sha256.is_empty());
        assert!(!hashes.sha512.is_empty());
        assert!(!hashes.blake3.is_empty());
    }

    #[tokio::test]
    async fn test_hash_caching() {
        let metrics = Arc::new(MetricsCollector::new());
        let config = HashConfig {
            enable_cache: true,
            ..Default::default()
        };
        let handler = HashHandler::new(config, metrics.clone());

        let test_data = b"Hello, World!";
        let test_file = create_test_file(test_data).await;
        let document = Document::new(test_file.path());

        // First computation
        let result1 = handler.compute_hashes(&document).await.unwrap();

        // Second computation should return cached result
        let result2 = handler.compute_hashes(&document).await.unwrap();

        assert_eq!(result1.md5, result2.md5);
        assert_eq!(result1.sha256, result2.sha256);
        assert_eq!(result1.sha512, result2.sha512);
        assert_eq!(result1.blake3, result2.blake3);
    }

    #[tokio::test]
    async fn test_concurrent_computations() {
        let metrics = Arc::new(MetricsCollector::new());
        let config = HashConfig {
            max_concurrent: 2,
            ..Default::default()
        };
        let handler = HashHandler::new(config, metrics.clone());

        let mut handles = Vec::new();
        let mut files = Vec::new();

        // Create 5 test files and start computations
        for i in 0..5 {
            let test_data = format!("Test data {}", i).into_bytes();
            let test_file = create_test_file(&test_data).await;
            let document = Document::new(test_file.path());
            files.push(test_file);

            let handler = handler.clone();
            handles.push(tokio::spawn(async move {
                handler.compute_hashes(&document).await
            }));
        }

        // Wait for all computations
        let results: Vec<_> = futures::future::join_all(handles).await;

        // Should have some successful and some failed due to concurrency limit
        let successful = results.iter().filter(|r| r.as_ref().unwrap().is_ok()).count();
        assert!(successful > 0 && successful < 5);
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let metrics = Arc::new(MetricsCollector::new());
        let config = HashConfig::default();
        let handler = HashHandler::new(config, metrics.clone());

        let test_data = b"Hello, World!";
        let test_file = create_test_file(test_data).await;
        let document = Document::new(test_file.path());

        let _ = handler.compute_hashes(&document).await;

        let counters = metrics.get_counters().await;
        assert_eq!(counters.get("hash_computations_started"), Some(&1));
        assert!(counters.get("hash_computations_completed").is_some());
        assert!(metrics.get_histogram("hash_computation_duration").await.count > 0);
    }
                                      }

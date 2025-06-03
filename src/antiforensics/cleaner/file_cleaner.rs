//! File Cleaner Implementation
//! Author: kartik4091
//! Created: 2025-06-03 08:57:02 UTC

use super::*;
use crate::utils::metrics::Metrics;
use std::{
    sync::Arc,
    path::PathBuf,
    time::{Duration, Instant},
    collections::{HashMap, HashSet},
    io::{self, SeekFrom},
};
use tokio::{
    sync::{RwLock, Semaphore, broadcast},
    fs::{self, File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt, AsyncSeekExt},
};
use tracing::{info, warn, error, debug, instrument};

/// File cleaner configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCleanerConfig {
    /// Base cleaner configuration
    pub base: CleanerConfig,
    /// File patterns to clean
    pub file_patterns: HashSet<String>,
    /// Content replacement patterns
    pub replacements: HashMap<Vec<u8>, Vec<u8>>,
    /// Zero fill slack space
    pub zero_slack: bool,
    /// Truncate after cleaning
    pub truncate: bool,
}

/// File cleaner state
#[derive(Debug)]
struct FileCleanerState {
    /// Active cleanings
    active_cleanings: HashSet<PathBuf>,
    /// Statistics
    stats: FileCleanStats,
}

/// File cleaning statistics
#[derive(Debug, Default)]
struct FileCleanStats {
    files_cleaned: u64,
    bytes_cleaned: u64,
    patterns_replaced: u64,
    avg_clean_time: Duration,
}

pub struct FileCleaner {
    /// Base cleaner
    base: Arc<BaseCleaner>,
    /// File cleaner configuration
    config: Arc<FileCleanerConfig>,
    /// Cleaner state
    state: Arc<RwLock<FileCleanerState>>,
    /// Performance metrics
    metrics: Arc<Metrics>,
}

impl FileCleaner {
    /// Creates a new file cleaner
    pub fn new(config: FileCleanerConfig) -> Self {
        Self {
            base: Arc::new(BaseCleaner::new(config.base.clone())),
            config: Arc::new(config),
            state: Arc::new(RwLock::new(FileCleanerState {
                active_cleanings: HashSet::new(),
                stats: FileCleanStats::default(),
            })),
            metrics: Arc::new(Metrics::new()),
        }
    }

    /// Clean file content
    #[instrument(skip(self, file))]
    async fn clean_content(&self, file: &mut File, size: u64) -> Result<u64> {
        let start = Instant::now();
        let mut cleaned_size = 0u64;
        let mut buffer = vec![0u8; self.config.base.buffer_size];

        file.seek(SeekFrom::Start(0)).await?;
        let mut position = 0u64;

        while position < size {
            // Read chunk
            let read_size = ((size - position) as usize)
                .min(self.config.base.buffer_size);
            let n = file.read(&mut buffer[..read_size]).await?;
            if n == 0 { break; }

            // Process content
            let mut modified = false;
            for (pattern, replacement) in &self.config.replacements {
                if let Some(idx) = find_pattern(&buffer[..n], pattern) {
                    replace_pattern(&mut buffer[..n], idx, pattern, replacement);
                    modified = true;
                }
            }

            // Write back if modified
            if modified {
                file.seek(SeekFrom::Start(position)).await?;
                file.write_all(&buffer[..n]).await?;
                cleaned_size += n as u64;
            }

            position += n as u64;
        }

        // Zero fill slack space if enabled
        if self.config.zero_slack {
            cleaned_size += self.zero_fill_slack(file, size).await?;
        }

        // Truncate if enabled
        if self.config.truncate {
            file.set_len(cleaned_size).await?;
        }

        self.metrics.record_operation("content_cleaning", start.elapsed()).await;
        Ok(cleaned_size)
    }

    /// Zero fills file slack space
    #[instrument(skip(self, file))]
    async fn zero_fill_slack(&self, file: &mut File, size: u64) -> Result<u64> {
        let start = Instant::now();
        let mut zeros = vec![0u8; self.config.base.buffer_size];
        let mut written = 0u64;

        // Get filesystem block size
        let block_size = self.get_block_size(file).await?;
        let padded_size = (size + block_size - 1) & !(block_size - 1);
        let slack_size = padded_size - size;

        if slack_size > 0 {
            file.seek(SeekFrom::Start(size)).await?;
            let mut remaining = slack_size;

            while remaining > 0 {
                let write_size = remaining.min(zeros.len() as u64) as usize;
                file.write_all(&zeros[..write_size]).await?;
                remaining -= write_size as u64;
                written += write_size as u64;
            }
        }

        self.metrics.record_operation("slack_cleaning", start.elapsed()).await;
        Ok(written)
    }

    /// Gets filesystem block size
    async fn get_block_size(&self, file: &File) -> Result<u64> {
        // Default to 4KB if we can't determine the actual block size
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::MetadataExt;
            Ok(file.metadata().await?.blksize() as u64)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(4096)
        }
    }

    /// Validates file pattern
    fn matches_pattern(&self, path: &PathBuf) -> bool {
        if let Some(ext) = path.extension() {
            self.config.file_patterns.contains(
                &ext.to_string_lossy().to_string()
            )
        } else {
            false
        }
    }
}

/// Find pattern in buffer
fn find_pattern(buffer: &[u8], pattern: &[u8]) -> Option<usize> {
    buffer.windows(pattern.len())
        .position(|window| window == pattern)
}

/// Replace pattern in buffer
fn replace_pattern(buffer: &mut [u8], start: usize, pattern: &[u8], replacement: &[u8]) {
    let end = start + pattern.len();
    if end <= buffer.len() {
        buffer[start..end].copy_from_slice(
            &replacement.iter()
                .chain(std::iter::repeat(&0))
                .take(pattern.len())
                .copied()
                .collect::<Vec<_>>()
        );
    }
}

#[async_trait]
impl Cleaner for FileCleaner {
    #[instrument(skip(self))]
    async fn clean_file(&self, path: &PathBuf) -> Result<CleanResult> {
        let start = Instant::now();

        // Get rate limiting permit
        let _permit = self.base.semaphore.acquire().await
            .map_err(|e| CleanerError::Internal(e.to_string()))?;

        // Validate input
        self.validate(path).await?;

        // Open file for reading and writing
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .await?;

        // Get original size
        let original_size = file.metadata().await?.len();

        // Clean content
        let cleaned_size = self.clean_content(&mut file, original_size).await?;

        // Secure overwrite if enabled
        if self.config.base.passes > 0 {
            self.base.secure_overwrite(&mut file, cleaned_size).await?;
        }

        // Update statistics
        let duration = start.elapsed();
        self.base.update_metrics(duration, true, cleaned_size).await;

        let mut state = self.state.write().await;
        state.stats.files_cleaned += 1;
        state.stats.bytes_cleaned += cleaned_size;
        state.stats.avg_clean_time = (state.stats.avg_clean_time + duration) / 2;

        let result = CleanResult {
            path: path.clone(),
            original_size,
            cleaned_size,
            duration,
            verified: self.config.base.verify,
            metrics: CleanMetrics {
                duration,
                memory_usage: self.config.base.buffer_size,
                write_ops: (cleaned_size / self.config.base.buffer_size as u64) + 1,
                bytes_written: cleaned_size,
            },
        };

        // Record history and notify subscribers
        self.base.record_history(path.clone(), duration, true).await;
        let _ = self.base.alert_tx.send(result.clone());

        Ok(result)
    }

    #[instrument(skip(self))]
    async fn validate(&self, path: &PathBuf) -> Result<()> {
        // Check if file exists
        if !path.exists() {
            return Err(CleanerError::InvalidInput(
                format!("File not found: {}", path.display())
            ));
        }

        // Check if file matches patterns
        if !self.matches_pattern(path) {
            return Err(CleanerError::InvalidInput(
                format!("Unsupported file type: {}", path.display())
            ));
        }

        // Check permissions
        let metadata = fs::metadata(path).await?;
        if metadata.permissions().readonly() {
            return Err(CleanerError::PermissionDenied(
                format!("File is read-only: {}", path.display())
            ));
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn cleanup(&self) -> Result<()> {
        let mut state = self.state.write().await;
        state.active_cleanings.clear();
        state.stats = FileCleanStats::default();
        Ok(())
    }

    #[instrument(skip(self))]
    async fn get_stats(&self) -> Result<CleanStats> {
        let state = self.state.read().await;
        Ok(CleanStats {
            total_ops: state.stats.files_cleaned,
            successful_ops: state.stats.files_cleaned,
            failed_ops: 0,
            total_bytes: state.stats.bytes_cleaned,
            avg_op_time: state.stats.avg_clean_time,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn create_test_config() -> FileCleanerConfig {
        FileCleanerConfig {
            base: CleanerConfig::default(),
            file_patterns: ["txt", "dat"].iter().map(|&s| s.into()).collect(),
            replacements: [
                (b"password".to_vec(), b"********".to_vec()),
            ].iter().cloned().collect(),
            zero_slack: true,
            truncate: true,
        }
    }

    #[tokio::test]
    async fn test_content_cleaning() {
        let cleaner = FileCleaner::new(create_test_config());
        let mut file = NamedTempFile::new().unwrap();
        
        // Write test data
        tokio::io::AsyncWriteExt::write_all(
            &mut File::from_std(file.reopen().unwrap()),
            b"test password data"
        ).await.unwrap();
        
        let mut async_file = File::from_std(file.reopen().unwrap());
        let size = async_file.metadata().await.unwrap().len();
        
        let cleaned_size = cleaner.clean_content(&mut async_file, size).await.unwrap();
        assert!(cleaned_size > 0);
    }

    #[tokio::test]
    async fn test_pattern_matching() {
        let cleaner = FileCleaner::new(create_test_config());
        
        assert!(cleaner.matches_pattern(&PathBuf::from("test.txt")));
        assert!(!cleaner.matches_pattern(&PathBuf::from("test.exe")));
    }

    #[tokio::test]
    async fn test_slack_space_cleaning() {
        let cleaner = FileCleaner::new(FileCleanerConfig {
            zero_slack: true,
            ..create_test_config()
        });
        let mut file = NamedTempFile::new().unwrap();
        let mut async_file = File::from_std(file.reopen().unwrap());
        
        async_file.write_all(b"test").await.unwrap();
        let written = cleaner.zero_fill_slack(&mut async_file, 4).await.unwrap();
        
        assert!(written > 0);
    }

    #[tokio::test]
    async fn test_concurrent_cleaning() {
        let cleaner = FileCleaner::new(FileCleanerConfig {
            base: CleanerConfig {
                max_concurrent_ops: 2,
                ..CleanerConfig::default()
            },
            ..create_test_config()
        });

        let files: Vec<_> = (0..4).map(|_| NamedTempFile::new().unwrap()).collect();
        let handles: Vec<_> = files.iter().map(|file| {
            let cleaner = cleaner.clone();
            let path = PathBuf::from(file.path());
            tokio::spawn(async move {
                cleaner.clean_file(&path).await
            })
        }).collect();

        let results = futures::future::join_all(handles).await;
        for result in results {
            assert!(result.unwrap().is_ok());
        }
    }

    #[tokio::test]
    async fn test_pattern_replacement() {
        let mut buffer = b"password123".to_vec();
        let pattern = b"password";
        let replacement = b"********";
        
        if let Some(idx) = find_pattern(&buffer, pattern) {
            replace_pattern(&mut buffer, idx, pattern, replacement);
            assert_eq!(&buffer[..8], b"********");
        }
    }
  }

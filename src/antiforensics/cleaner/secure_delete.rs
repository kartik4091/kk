//! Secure Delete Implementation
//! Author: kartik4091
//! Created: 2025-06-03 09:03:55 UTC

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

/// Secure delete configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureDeleteConfig {
    /// Base cleaner configuration
    pub base: CleanerConfig,
    /// Wipe method
    pub wipe_method: WipeMethod,
    /// Rename before deletion
    pub rename_before_delete: bool,
    /// Number of renames
    pub rename_count: usize,
    /// Delete empty directories
    pub delete_empty_dirs: bool,
}

/// Wipe methods
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum WipeMethod {
    /// Single pass zeros
    Zeros,
    /// Single pass random
    Random,
    /// DoD 5220.22-M (3 passes)
    Dod,
    /// Gutmann (35 passes)
    Gutmann,
    /// Custom pattern sequence
    Custom(Vec<Vec<u8>>),
}

/// Secure delete state
#[derive(Debug)]
struct SecureDeleteState {
    /// Active deletions
    active_deletions: HashSet<PathBuf>,
    /// Statistics
    stats: SecureDeleteStats,
}

/// Secure delete statistics
#[derive(Debug, Default)]
struct SecureDeleteStats {
    files_deleted: u64,
    bytes_processed: u64,
    dirs_cleaned: u64,
    avg_delete_time: Duration,
}

pub struct SecureDelete {
    /// Base cleaner
    base: Arc<BaseCleaner>,
    /// Secure delete configuration
    config: Arc<SecureDeleteConfig>,
    /// Cleaner state
    state: Arc<RwLock<SecureDeleteState>>,
    /// Performance metrics
    metrics: Arc<Metrics>,
}

impl SecureDelete {
    /// Creates a new secure delete instance
    pub fn new(config: SecureDeleteConfig) -> Self {
        Self {
            base: Arc::new(BaseCleaner::new(config.base.clone())),
            config: Arc::new(config),
            state: Arc::new(RwLock::new(SecureDeleteState {
                active_deletions: HashSet::new(),
                stats: SecureDeleteStats::default(),
            })),
            metrics: Arc::new(Metrics::new()),
        }
    }

    /// Gets wipe patterns for selected method
    fn get_wipe_patterns(&self) -> Vec<Vec<u8>> {
        match self.config.wipe_method {
            WipeMethod::Zeros => vec![vec![0x00]],
            WipeMethod::Random => vec![rand::random::<u8>().to_vec()],
            WipeMethod::Dod => vec![
                vec![0x00],
                vec![0xFF],
                vec![rand::random::<u8>()],
            ],
            WipeMethod::Gutmann => {
                let mut patterns = Vec::with_capacity(35);
                // Patterns 1-4: Random
                for _ in 0..4 {
                    patterns.push(rand::random::<u8>().to_vec());
                }
                // Patterns 5-31: Fixed patterns
                patterns.extend_from_slice(&[
                    vec![0x55], vec![0xAA], vec![0x92, 0x49, 0x24],
                    vec![0x49, 0x24, 0x92], vec![0x24, 0x92, 0x49],
                    vec![0x00], vec![0x11], vec![0x22], vec![0x33],
                    vec![0x44], vec![0x55], vec![0x66], vec![0x77],
                    vec![0x88], vec![0x99], vec![0xAA], vec![0xBB],
                    vec![0xCC], vec![0xDD], vec![0xEE], vec![0xFF],
                    vec![0x92, 0x49, 0x24], vec![0x49, 0x24, 0x92],
                    vec![0x24, 0x92, 0x49], vec![0x6D, 0xB6, 0xDB],
                    vec![0xB6, 0xDB, 0x6D], vec![0xDB, 0x6D, 0xB6],
                ]);
                // Patterns 32-35: Random
                for _ in 0..4 {
                    patterns.push(rand::random::<u8>().to_vec());
                }
                patterns
            },
            WipeMethod::Custom(patterns) => patterns,
        }
    }

    /// Generates random filename
    fn generate_random_name(&self) -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        let mut rng = rand::thread_rng();
        let name: String = (0..12)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();
        name
    }

    /// Renames file multiple times before deletion
    #[instrument(skip(self, file))]
    async fn secure_rename(&self, file: &PathBuf) -> Result<()> {
        let parent = file.parent()
            .ok_or_else(|| CleanerError::InvalidInput("Invalid file path".into()))?;

        for _ in 0..self.config.rename_count {
            let new_name = parent.join(self.generate_random_name());
            fs::rename(file, &new_name).await?;
            tokio::time::sleep(Duration::from_millis(10)).await;
            fs::rename(&new_name, file).await?;
        }
        Ok(())
    }

    /// Checks if directory is empty
    async fn is_directory_empty(&self, path: &PathBuf) -> Result<bool> {
        let mut entries = fs::read_dir(path).await?;
        Ok(entries.next_entry().await?.is_none())
    }

    /// Recursively deletes empty directories
    #[instrument(skip(self))]
    async fn cleanup_empty_dirs(&self, path: &PathBuf) -> Result<u64> {
        let mut count = 0;

        if path.is_dir() {
            let mut entries = fs::read_dir(path).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.is_dir() {
                    count += self.cleanup_empty_dirs(&path).await?;
                }
            }

            if self.is_directory_empty(path).await? {
                fs::remove_dir(path).await?;
                count += 1;
            }
        }

        Ok(count)
    }
}

#[async_trait]
impl Cleaner for SecureDelete {
    #[instrument(skip(self))]
    async fn clean_file(&self, path: &PathBuf) -> Result<CleanResult> {
        let start = Instant::now();

        // Get rate limiting permit
        let _permit = self.base.semaphore.acquire().await
            .map_err(|e| CleanerError::Internal(e.to_string()))?;

        // Validate input
        self.validate(path).await?;

        // Get file size
        let metadata = fs::metadata(path).await?;
        let file_size = metadata.len();

        // Open file for overwriting
        let mut file = OpenOptions::new()
            .write(true)
            .open(path)
            .await?;

        // Get wipe patterns
        let patterns = self.get_wipe_patterns();

        // Perform secure overwrite
        for pattern in &patterns {
            self.base.secure_overwrite(&mut file, file_size).await?;
        }

        // Rename file if configured
        if self.config.rename_before_delete {
            self.secure_rename(path).await?;
        }

        // Delete file
        drop(file);
        fs::remove_file(path).await?;

        // Cleanup empty directories if configured
        let mut dirs_cleaned = 0;
        if self.config.delete_empty_dirs {
            if let Some(parent) = path.parent() {
                dirs_cleaned = self.cleanup_empty_dirs(parent).await?;
            }
        }

        // Update statistics
        let duration = start.elapsed();
        self.base.update_metrics(duration, true, file_size).await;

        let mut state = self.state.write().await;
        state.stats.files_deleted += 1;
        state.stats.bytes_processed += file_size;
        state.stats.dirs_cleaned += dirs_cleaned;
        state.stats.avg_delete_time = (state.stats.avg_delete_time + duration) / 2;

        let result = CleanResult {
            path: path.clone(),
            original_size: file_size,
            cleaned_size: 0,
            duration,
            verified: self.config.base.verify,
            metrics: CleanMetrics {
                duration,
                memory_usage: self.config.base.buffer_size,
                write_ops: patterns.len() as u64,
                bytes_written: file_size * patterns.len() as u64,
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

        // Check if it's a file
        if !path.is_file() {
            return Err(CleanerError::InvalidInput(
                format!("Not a file: {}", path.display())
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
        state.active_deletions.clear();
        state.stats = SecureDeleteStats::default();
        Ok(())
    }

    #[instrument(skip(self))]
    async fn get_stats(&self) -> Result<CleanStats> {
        let state = self.state.read().await;
        Ok(CleanStats {
            total_ops: state.stats.files_deleted,
            successful_ops: state.stats.files_deleted,
            failed_ops: 0,
            total_bytes: state.stats.bytes_processed,
            avg_op_time: state.stats.avg_delete_time,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::{NamedTempFile, TempDir};

    fn create_test_config() -> SecureDeleteConfig {
        SecureDeleteConfig {
            base: CleanerConfig::default(),
            wipe_method: WipeMethod::Dod,
            rename_before_delete: true,
            rename_count: 3,
            delete_empty_dirs: true,
        }
    }

    #[tokio::test]
    async fn test_secure_overwrite() {
        let deleter = SecureDelete::new(create_test_config());
        let file = NamedTempFile::new().unwrap();
        let path = PathBuf::from(file.path());
        
        tokio::fs::write(&path, b"test data").await.unwrap();
        deleter.clean_file(&path).await.unwrap();
        
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn test_wipe_patterns() {
        let deleter = SecureDelete::new(SecureDeleteConfig {
            wipe_method: WipeMethod::Dod,
            ..create_test_config()
        });
        
        let patterns = deleter.get_wipe_patterns();
        assert_eq!(patterns.len(), 3);
    }

    #[tokio::test]
    async fn test_directory_cleanup() {
        let deleter = SecureDelete::new(SecureDeleteConfig {
            delete_empty_dirs: true,
            ..create_test_config()
        });
        
        let temp_dir = TempDir::new().unwrap();
        let dir_path = PathBuf::from(temp_dir.path());
        
        let empty_count = deleter.cleanup_empty_dirs(&dir_path).await.unwrap();
        assert_eq!(empty_count, 0);
    }

    #[tokio::test]
    async fn test_concurrent_deletion() {
        let deleter = SecureDelete::new(SecureDeleteConfig {
            base: CleanerConfig {
                max_concurrent_ops: 2,
                ..CleanerConfig::default()
            },
            ..create_test_config()
        });

        let files: Vec<_> = (0..4).map(|_| NamedTempFile::new().unwrap()).collect();
        let handles: Vec<_> = files.iter().map(|file| {
            let deleter = deleter.clone();
            let path = PathBuf::from(file.path());
            tokio::spawn(async move {
                deleter.clean_file(&path).await
            })
        }).collect();

        let results = futures::future::join_all(handles).await;
        for result in results {
            assert!(result.unwrap().is_ok());
        }
    }

    #[tokio::test]
    async fn test_renaming() {
        let deleter = SecureDelete::new(SecureDeleteConfig {
            rename_before_delete: true,
            rename_count: 5,
            ..create_test_config()
        });
        let file = NamedTempFile::new().unwrap();
        let path = PathBuf::from(file.path());
        
        assert!(deleter.secure_rename(&path).await.is_ok());
    }

    #[tokio::test]
    async fn test_stats_tracking() {
        let deleter = SecureDelete::new(create_test_config());
        let file = NamedTempFile::new().unwrap();
        let path = PathBuf::from(file.path());
        
        tokio::fs::write(&path, b"test data").await.unwrap();
        deleter.clean_file(&path).await.unwrap();
        
        let stats = deleter.get_stats().await.unwrap();
        assert_eq!(stats.total_ops, 1);
        assert!(stats.total_bytes > 0);
    }
              }

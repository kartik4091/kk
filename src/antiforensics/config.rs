//! Configuration types for PDF anti-forensics operations
//! Author: kartik4091
//! Created: 2025-06-03 10:19:43 UTC
//! This module defines configuration structures used throughout the system.

use std::{
    path::PathBuf,
    time::Duration,
};
use serde::{Deserialize, Serialize};
use crate::types::{
    VerificationLevel,
    UserMetadata,
    SecurityOptions,
};

/// Main processing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingConfig {
    /// Input PDF path
    pub input_path: PathBuf,
    /// Output PDF path
    pub output_path: PathBuf,
    /// Verification level
    pub verification_level: VerificationLevel,
    /// User metadata
    pub user_metadata: Option<UserMetadata>,
    /// Security options
    pub security_options: Option<SecurityOptions>,
    /// Performance options
    pub performance: PerformanceConfig,
    /// Cleanup options
    pub cleanup: CleanupConfig,
}

/// Performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Maximum threads
    pub max_threads: usize,
    /// Batch size
    pub batch_size: usize,
    /// Operation timeout
    pub operation_timeout: Duration,
    /// Memory limit (bytes)
    pub memory_limit: u64,
    /// Enable caching
    pub enable_cache: bool,
}

/// Cleanup configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupConfig {
    /// Clean streams
    pub clean_streams: bool,
    /// Clean metadata
    pub clean_metadata: bool,
    /// Clean structure
    pub clean_structure: bool,
    /// Secure delete
    pub secure_delete: bool,
    /// Compression options
    pub compression: CompressionConfig,
}

/// Compression configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// Enable compression
    pub enable_compression: bool,
    /// Compression level (0-9)
    pub compression_level: u8,
    /// Compression method
    pub compression_method: CompressionMethod,
}

/// Compression methods
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionMethod {
    /// Deflate compression
    Deflate,
    /// LZW compression
    Lzw,
    /// Run Length encoding
    RunLength,
}

impl Default for ProcessingConfig {
    fn default() -> Self {
        Self {
            input_path: PathBuf::new(),
            output_path: PathBuf::new(),
            verification_level: VerificationLevel::Standard,
            user_metadata: None,
            security_options: None,
            performance: PerformanceConfig::default(),
            cleanup: CleanupConfig::default(),
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            max_threads: num_cpus::get(),
            batch_size: 1000,
            operation_timeout: Duration::from_secs(300),
            memory_limit: 1024 * 1024 * 1024, // 1GB
            enable_cache: true,
        }
    }
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            clean_streams: true,
            clean_metadata: true,
            clean_structure: true,
            secure_delete: true,
            compression: CompressionConfig::default(),
        }
    }
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            enable_compression: true,
            compression_level: 6,
            compression_method: CompressionMethod::Deflate,
        }
    }
}

impl ProcessingConfig {
    /// Creates a new processing configuration
    pub fn new(
        input_path: PathBuf,
        output_path: PathBuf,
        verification_level: VerificationLevel,
    ) -> Self {
        Self {
            input_path,
            output_path,
            verification_level,
            ..Default::default()
        }
    }

    /// Validates the configuration
    pub fn validate(&self) -> crate::error::Result<()> {
        // Validate paths
        if !self.input_path.exists() {
            return Err(crate::error::ForensicError::Config(
                "Input file does not exist".to_string(),
            ));
        }

        // Validate compression level
        if self.cleanup.compression.compression_level > 9 {
            return Err(crate::error::ForensicError::Config(
                "Invalid compression level".to_string(),
            ));
        }

        // Validate performance settings
        if self.performance.max_threads == 0 {
            return Err(crate::error::ForensicError::Config(
                "Max threads must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_config() {
        let config = ProcessingConfig::default();
        assert_eq!(config.verification_level, VerificationLevel::Standard);
        assert!(config.user_metadata.is_none());
        assert!(config.security_options.is_none());
    }

    #[test]
    fn test_config_validation() {
        let temp_file = NamedTempFile::new().unwrap();
        let config = ProcessingConfig::new(
            temp_file.path().to_path_buf(),
            PathBuf::from("output.pdf"),
            VerificationLevel::Standard,
        );
        assert!(config.validate().is_ok());

        let invalid_config = ProcessingConfig::new(
            PathBuf::from("nonexistent.pdf"),
            PathBuf::from("output.pdf"),
            VerificationLevel::Standard,
        );
        assert!(invalid_config.validate().is_err());
    }

    #[test]
    fn test_performance_config() {
        let config = PerformanceConfig::default();
        assert_eq!(config.max_threads, num_cpus::get());
        assert_eq!(config.batch_size, 1000);
        assert!(config.enable_cache);
    }

    #[test]
    fn test_compression_config() {
        let config = CompressionConfig::default();
        assert!(config.enable_compression);
        assert_eq!(config.compression_level, 6);
        assert_eq!(config.compression_method, CompressionMethod::Deflate);
    }
      }

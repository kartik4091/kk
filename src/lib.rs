use std::{collections::BTreeMap, sync::Arc};
use thiserror::Error;
use uuid::Uuid;

pub mod core;
pub mod security;
pub mod verification;
pub mod writer;
pub mod metrics;
pub mod utils;

#[derive(Error, Debug)]
pub enum PdfError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("PDF processing error: {0}")]
    Processing(String),
    
    #[error("Security error: {0}")]
    Security(String),
    
    #[error("Validation error: {0}")]
    Validation(String),
    
    #[error("Configuration error: {0}")]
    Configuration(String),
    
    #[error("Encryption error: {0}")]
    Encryption(String),
    
    #[error("Compression error: {0}")]
    Compression(String),
}

pub struct ProcessingOptions {
    pub optimize: bool,
    pub compress: bool,
    pub encrypt: bool,
    pub validate: bool,
    pub sign: bool,
}

impl Default for ProcessingOptions {
    fn default() -> Self {
        Self {
            optimize: true,
            compress: true,
            encrypt: false,
            validate: true,
            sign: false,
        }
    }
}

#[derive(Debug)]
pub struct ProcessingResult {
    pub document_id: String,
    pub processed_bytes: usize,
    pub compression_ratio: f64,
    pub processing_time: std::time::Duration,
    pub status: ProcessingStatus,
}

#[derive(Debug)]
pub enum ProcessingStatus {
    Success,
    PartialSuccess(String),
    Failed(String),
}

#[derive(Clone)]
pub struct EngineConfig {
    pub max_concurrent_jobs: usize,
    pub buffer_size: usize,
    pub temp_dir: std::path::PathBuf,
    pub metrics_enabled: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_concurrent_jobs: num_cpus::get(),
            buffer_size: 8 * 1024 * 1024, // 8MB
            temp_dir: std::env::temp_dir(),
            metrics_enabled: true,
        }
    }
}

pub struct PdfEngine {
    config: EngineConfig,
    core: Arc<core::CoreSystem>,
    writer: Arc<writer::WriterSystem>,
    security: Arc<security::SecuritySystem>,
    verification: Arc<verification::VerificationSystem>,
    metrics: Arc<metrics::MetricsRegistry>,
}

impl PdfEngine {
    pub async fn new(config: Option<EngineConfig>) -> Result<Self, PdfError> {
        let config = config.unwrap_or_default();
        let metrics = if config.metrics_enabled {
            Arc::new(metrics::MetricsRegistry::new()?)
        } else {
            Arc::new(metrics::MetricsRegistry::disabled())
        };

        let core = Arc::new(core::CoreSystem::new(&config, metrics.clone()).await?);
        let writer = Arc::new(writer::WriterSystem::new(&config, metrics.clone()).await?);
        let security = Arc::new(security::SecuritySystem::new(&config, metrics.clone()).await?);
        let verification = Arc::new(verification::VerificationSystem::new(&config, metrics.clone()).await?);

        Ok(Self {
            config,
            core,
            writer,
            security,
            verification,
            metrics,
        })
    }

    pub async fn process_document(
        &self,
        input: &[u8],
        options: Option<ProcessingOptions>
    ) -> Result<ProcessingResult, PdfError> {
        let start_time = std::time::Instant::now();
        let document_id = Uuid::new_v4().to_string();
        let options = options.unwrap_or_default();

        // Track active jobs
        self.metrics.active_operations.inc();
        let _timer = self.metrics.processing_duration.start_timer();

        let result = self.internal_process_document(input, &document_id, &options).await;

        // Update metrics
        self.metrics.active_operations.dec();
        self.metrics.documents_processed.inc();
        self.metrics.bytes_processed.inc_by(input.len() as f64);

        match result {
            Ok(processed_data) => {
                let compression_ratio = if input.len() > 0 {
                    processed_data.len() as f64 / input.len() as f64
                } else {
                    1.0
                };

                Ok(ProcessingResult {
                    document_id,
                    processed_bytes: processed_data.len(),
                    compression_ratio,
                    processing_time: start_time.elapsed(),
                    status: ProcessingStatus::Success,
                })
            }
            Err(e) => {
                self.metrics.processing_errors.inc();
                Ok(ProcessingResult {
                    document_id,
                    processed_bytes: 0,
                    compression_ratio: 1.0,
                    processing_time: start_time.elapsed(),
                    status: ProcessingStatus::Failed(e.to_string()),
                })
            }
        }
    }

    async fn internal_process_document(
        &self,
        input: &[u8],
        document_id: &str,
        options: &ProcessingOptions,
    ) -> Result<Vec<u8>, PdfError> {
        // Step 1: Validation
        if options.validate {
            let verification_result = self.verification.verify_document(input).await?;
            if !verification_result.is_valid {
                return Err(PdfError::Validation(verification_result.message));
            }
        }

        // Step 2: Security checks
        let security_result = self.security.check_document(input).await?;
        if !security_result.is_secure {
            return Err(PdfError::Security(security_result.message));
        }

        // Step 3: Core processing
        let mut processed_data = self.core.process_document(input).await?;

        // Step 4: Optimization
        if options.optimize {
            processed_data = self.writer.optimize_document(&processed_data).await?;
        }

        // Step 5: Compression
        if options.compress {
            processed_data = self.writer.compress_document(&processed_data).await?;
        }

        // Step 6: Encryption
        if options.encrypt {
            processed_data = self.security.encrypt_document(&processed_data).await?;
        }

        // Step 7: Digital Signature
        if options.sign {
            processed_data = self.security.sign_document(&processed_data).await?;
        }

        Ok(processed_data)
    }

    pub fn metrics(&self) -> Arc<metrics::MetricsRegistry> {
        self.metrics.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::fs;

    #[tokio::test]
    async fn test_pdf_engine_creation() {
        let engine = PdfEngine::new(None).await;
        assert!(engine.is_ok());
    }

    #[tokio::test]
    async fn test_pdf_processing() {
        let engine = PdfEngine::new(None).await.unwrap();
        let sample_pdf = include_bytes!("../tests/data/sample.pdf");
        let result = engine.process_document(sample_pdf, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_pdf_optimization() {
        let engine = PdfEngine::new(None).await.unwrap();
        let sample_pdf = include_bytes!("../tests/data/sample.pdf");
        
        let options = ProcessingOptions {
            optimize: true,
            compress: true,
            ..Default::default()
        };

        let result = engine.process_document(sample_pdf, Some(options)).await.unwrap();
        assert!(matches!(result.status, ProcessingStatus::Success));
        assert!(result.compression_ratio < 1.0);
    }

    #[tokio::test]
    async fn test_pdf_encryption() {
        let engine = PdfEngine::new(None).await.unwrap();
        let sample_pdf = include_bytes!("../tests/data/sample.pdf");
        
        let options = ProcessingOptions {
            encrypt: true,
            ..Default::default()
        };

        let result = engine.process_document(sample_pdf, Some(options)).await.unwrap();
        assert!(matches!(result.status, ProcessingStatus::Success));
    }
}

pub use chrono::{DateTime, Utc};
pub use lopdf::Document;
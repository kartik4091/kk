use crate::{metrics::MetricsRegistry, EngineConfig, PdfError};
use chrono::{DateTime, Utc};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use lopdf::{Document, Object, ObjectId, Stream, Dictionary};

pub mod compression;
pub mod metadata;
pub mod optimization;
pub mod stream;
pub mod xref;
pub mod validation;

pub struct WriterSystem {
    state: Arc<RwLock<WriterState>>,
    config: WriterConfig,
    metrics: Arc<MetricsRegistry>,
    compression: Arc<compression::CompressionSystem>,
    optimization: Arc<optimization::OptimizationSystem>,
    validation: Arc<validation::ValidationSystem>,
}

struct WriterState {
    documents_written: u64,
    last_write: Option<DateTime<Utc>>,
    active_writers: u32,
    bytes_written: u64,
    compression_ratios: Vec<f64>,
}

#[derive(Clone)]
pub struct WriterConfig {
    pub compression_level: compression::CompressionLevel,
    pub optimization_level: optimization::OptimizationLevel,
    pub buffer_size: usize,
    pub max_concurrent_writers: usize,
    pub enable_incremental_update: bool,
}

#[derive(Debug)]
pub struct WriteOptions {
    pub compress: bool,
    pub optimize: bool,
    pub validate: bool,
    pub update_metadata: bool,
}

#[derive(Debug)]
pub struct WriteResult {
    pub document_id: String,
    pub bytes_written: usize,
    pub compression_ratio: f64,
    pub processing_time: std::time::Duration,
}

impl WriterSystem {
    pub async fn new(
        engine_config: &EngineConfig,
        metrics: Arc<MetricsRegistry>,
    ) -> Result<Self, PdfError> {
        let config = WriterConfig::default();

        let compression = Arc::new(compression::CompressionSystem::new(
            &config,
            metrics.clone(),
        ).await?);

        let optimization = Arc::new(optimization::OptimizationSystem::new(
            &config,
            metrics.clone(),
        ).await?);

        let validation = Arc::new(validation::ValidationSystem::new(
            &config,
            metrics.clone(),
        ).await?);

        Ok(Self {
            state: Arc::new(RwLock::new(WriterState {
                documents_written: 0,
                last_write: None,
                active_writers: 0,
                bytes_written: 0,
                compression_ratios: Vec::new(),
            })),
            config,
            metrics,
            compression,
            optimization,
            validation,
        })
    }

    pub async fn write_document(
        &self,
        data: &[u8],
        options: Option<WriteOptions>,
    ) -> Result<WriteResult, PdfError> {
        let start_time = std::time::Instant::now();
        let options = options.unwrap_or_default();

        // Update state
        {
            let mut state = self.state.write().map_err(|_| 
                PdfError::Processing("Failed to acquire state lock".to_string()))?;
            state.active_writers += 1;
        }

        let result = self.internal_write_document(data, &options).await;

        // Update metrics and state
        {
            let mut state = self.state.write().map_err(|_| 
                PdfError::Processing("Failed to acquire state lock".to_string()))?;
            state.active_writers -= 1;
            
            if let Ok(ref write_result) = result {
                state.documents_written += 1;
                state.bytes_written += write_result.bytes_written as u64;
                state.compression_ratios.push(write_result.compression_ratio);
                state.last_write = Some(Utc::now());
            }
        }

        result
    }

    async fn internal_write_document(
        &self,
        data: &[u8],
        options: &WriteOptions,
    ) -> Result<WriteResult, PdfError> {
        let start_time = std::time::Instant::now();
        let mut doc = Document::load_mem(data)
            .map_err(|e| PdfError::Processing(format!("Failed to load PDF: {}", e)))?;

        // Validate document if required
        if options.validate {
            self.validation.validate_document(&doc).await?;
        }

        // Optimize document if required
        if options.optimize {
            doc = self.optimization.optimize_document(doc).await?;
        }

        // Update metadata if required
        if options.update_metadata {
            self.update_document_metadata(&mut doc)?;
        }

        // Compress document if required
        let final_data = if options.compress {
            self.compression.compress_document(&doc).await?
        } else {
            let mut buffer = Vec::new();
            doc.save_to(&mut buffer)
                .map_err(|e| PdfError::Processing(format!("Failed to save PDF: {}", e)))?;
            buffer
        };

        let compression_ratio = if data.len() > 0 {
            final_data.len() as f64 / data.len() as f64
        } else {
            1.0
        };

        // Record metrics
        self.metrics.compression_ratio.observe(compression_ratio);
        self.metrics.bytes_processed.inc_by(final_data.len() as f64);

        Ok(WriteResult {
            document_id: uuid::Uuid::new_v4().to_string(),
            bytes_written: final_data.len(),
            compression_ratio,
            processing_time: start_time.elapsed(),
        })
    }

    fn update_document_metadata(&self, doc: &mut Document) -> Result<(), PdfError> {
        let info_dict = Dictionary::from_iter(vec![
            ("Producer", Object::string("PDF Engine 1.0")),
            ("ModDate", Object::string(Utc::now().to_rfc3339())),
            ("Creator", Object::string("kartik4091")),
        ]);

        let info_id = doc.add_object(info_dict);
        doc.trailer.set("Info", Object::Reference(info_id));

        Ok(())
    }

    pub async fn optimize_document(&self, data: &[u8]) -> Result<Vec<u8>, PdfError> {
        let doc = Document::load_mem(data)
            .map_err(|e| PdfError::Processing(format!("Failed to load PDF: {}", e)))?;

        let optimized_doc = self.optimization.optimize_document(doc).await?;

        let mut buffer = Vec::new();
        optimized_doc.save_to(&mut buffer)
            .map_err(|e| PdfError::Processing(format!("Failed to save PDF: {}", e)))?;

        Ok(buffer)
    }

    pub async fn compress_document(&self, data: &[u8]) -> Result<Vec<u8>, PdfError> {
        let doc = Document::load_mem(data)
            .map_err(|e| PdfError::Processing(format!("Failed to load PDF: {}", e)))?;

        self.compression.compress_document(&doc).await
    }
}

impl Default for WriterConfig {
    fn default() -> Self {
        Self {
            compression_level: compression::CompressionLevel::Default,
            optimization_level: optimization::OptimizationLevel::Standard,
            buffer_size: 8 * 1024 * 1024, // 8MB
            max_concurrent_writers: num_cpus::get(),
            enable_incremental_update: true,
        }
    }
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            compress: true,
            optimize: true,
            validate: true,
            update_metadata: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_writer_system_creation() {
        let config = EngineConfig::default();
        let metrics = Arc::new(MetricsRegistry::new().unwrap());
        let system = WriterSystem::new(&config, metrics).await;
        assert!(system.is_ok());
    }

    #[tokio::test]
    async fn test_document_writing() {
        let config = EngineConfig::default();
        let metrics = Arc::new(MetricsRegistry::new().unwrap());
        let system = WriterSystem::new(&config, metrics).await.unwrap();
        
        let sample_data = include_bytes!("../../tests/data/sample.pdf");
        let result = system.write_document(sample_data, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_document_optimization() {
        let config = EngineConfig::default();
        let metrics = Arc::new(MetricsRegistry::new().unwrap());
        let system = WriterSystem::new(&config, metrics).await.unwrap();
        
        let sample_data = include_bytes!("../../tests/data/sample.pdf");
        let result = system.optimize_document(sample_data).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_compression() {
        let config = EngineConfig::default();
        let metrics = Arc::new(MetricsRegistry::new().unwrap());
        let system = WriterSystem::new(&config, metrics).await.unwrap();
        
        let sample_data = include_bytes!("../../tests/data/sample.pdf");
        let result = system.compress_document(sample_data).await;
        assert!(result.is_ok());
        assert!(result.unwrap().len() <= sample_data.len());
    }
}
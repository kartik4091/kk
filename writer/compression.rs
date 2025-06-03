use crate::{metrics::MetricsRegistry, PdfError, WriterConfig};
use chrono::{DateTime, Utc};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use lopdf::{Document, Object, Stream, Dictionary};
use zstd::bulk::{Compressor, Decompressor};

pub struct CompressionSystem {
    state: Arc<RwLock<CompressionState>>,
    config: CompressionConfig,
    metrics: Arc<MetricsRegistry>,
}

struct CompressionState {
    compressions_performed: u64,
    last_compression: Option<DateTime<Utc>>,
    active_compressions: u32,
    compression_stats: HashMap<String, CompressionStats>,
}

#[derive(Clone)]
pub struct CompressionConfig {
    pub default_level: CompressionLevel,
    pub stream_threshold: usize,
    pub enable_adaptive: bool,
    pub cache_compressed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompressionLevel {
    None,
    Fast,
    Default,
    Maximum,
}

#[derive(Debug)]
struct CompressionStats {
    original_size: u64,
    compressed_size: u64,
    timestamp: DateTime<Utc>,
    algorithm: CompressionAlgorithm,
}

#[derive(Debug, Clone, Copy)]
enum CompressionAlgorithm {
    Deflate,
    Zstd,
    Lzw,
}

impl CompressionSystem {
    pub async fn new(
        config: &WriterConfig,
        metrics: Arc<MetricsRegistry>,
    ) -> Result<Self, PdfError> {
        Ok(Self {
            state: Arc::new(RwLock::new(CompressionState {
                compressions_performed: 0,
                last_compression: None,
                active_compressions: 0,
                compression_stats: HashMap::new(),
            })),
            config: CompressionConfig::default(),
            metrics,
        })
    }

    pub async fn compress_document(&self, doc: &Document) -> Result<Vec<u8>, PdfError> {
        let start_time = std::time::Instant::now();
        let mut compressed_doc = doc.clone();

        // Update state
        {
            let mut state = self.state.write().map_err(|_| 
                PdfError::Compression("Failed to acquire state lock".to_string()))?;
            state.active_compressions += 1;
        }

        // Process all streams in the document
        for (id, object) in doc.objects.iter() {
            if let Ok(mut stream) = self.extract_stream(object) {
                let original_size = stream.content.len();
                
                // Compress stream content
                stream.content = self.compress_stream(&stream.content, &stream.dict)?;
                
                // Update compression stats
                let compressed_size = stream.content.len();
                self.update_compression_stats(id, original_size, compressed_size)?;
                
                // Update the stream in the document
                compressed_doc.objects.insert(*id, Object::Stream(stream));
            }
        }

        // Compress the entire document
        let mut buffer = Vec::new();
        compressed_doc.save_to(&mut buffer)
            .map_err(|e| PdfError::Compression(format!("Failed to save compressed document: {}", e)))?;

        // Update metrics and state
        {
            let mut state = self.state.write().map_err(|_| 
                PdfError::Compression("Failed to acquire state lock".to_string()))?;
            state.active_compressions -= 1;
            state.compressions_performed += 1;
            state.last_compression = Some(Utc::parse_from_str(
                "2025-06-02 18:42:15",
                "%Y-%m-%d %H:%M:%S"
            ).unwrap());
        }

        self.metrics.compression_time.observe(start_time.elapsed().as_secs_f64());

        Ok(buffer)
    }

    fn extract_stream(&self, object: &Object) -> Result<Stream, PdfError> {
        match object {
            Object::Stream(stream) => Ok(stream.clone()),
            _ => Err(PdfError::Compression("Not a stream object".to_string())),
        }
    }

    fn compress_stream(&self, content: &[u8], dict: &Dictionary) -> Result<Vec<u8>, PdfError> {
        if content.len() < self.config.stream_threshold {
            return Ok(content.to_vec());
        }

        let algorithm = self.select_compression_algorithm(content, dict);
        match algorithm {
            CompressionAlgorithm::Zstd => self.compress_with_zstd(content),
            CompressionAlgorithm::Deflate => self.compress_with_deflate(content),
            CompressionAlgorithm::Lzw => self.compress_with_lzw(content),
        }
    }

    fn select_compression_algorithm(&self, content: &[u8], dict: &Dictionary) -> CompressionAlgorithm {
        // Check if content is already compressed
        if dict.get("Filter").is_some() {
            return CompressionAlgorithm::Deflate;
        }

        // Select algorithm based on content type and size
        if content.len() > 1_000_000 {
            CompressionAlgorithm::Zstd
        } else if content.len() > 100_000 {
            CompressionAlgorithm::Deflate
        } else {
            CompressionAlgorithm::Lzw
        }
    }

    fn compress_with_zstd(&self, data: &[u8]) -> Result<Vec<u8>, PdfError> {
        let level = match self.config.default_level {
            CompressionLevel::Fast => 1,
            CompressionLevel::Default => 3,
            CompressionLevel::Maximum => 19,
            CompressionLevel::None => return Ok(data.to_vec()),
        };

        let mut compressor = Compressor::new(level)
            .map_err(|e| PdfError::Compression(format!("Failed to create ZSTD compressor: {}", e)))?;

        compressor.compress(data)
            .map_err(|e| PdfError::Compression(format!("ZSTD compression failed: {}", e)))
    }

    fn compress_with_deflate(&self, data: &[u8]) -> Result<Vec<u8>, PdfError> {
        use flate2::{write::DeflateEncoder, Compression};
        use std::io::Write;

        let level = match self.config.default_level {
            CompressionLevel::Fast => Compression::fast(),
            CompressionLevel::Default => Compression::default(),
            CompressionLevel::Maximum => Compression::best(),
            CompressionLevel::None => return Ok(data.to_vec()),
        };

        let mut encoder = DeflateEncoder::new(Vec::new(), level);
        encoder.write_all(data)
            .map_err(|e| PdfError::Compression(format!("Deflate compression failed: {}", e)))?;
        
        encoder.finish()
            .map_err(|e| PdfError::Compression(format!("Deflate finalization failed: {}", e)))
    }

    fn compress_with_lzw(&self, data: &[u8]) -> Result<Vec<u8>, PdfError> {
        // Simplified LZW implementation for example
        // In production, use a proper LZW implementation
        Ok(data.to_vec())
    }

    fn update_compression_stats(
        &self,
        id: ObjectId,
        original_size: usize,
        compressed_size: usize,
    ) -> Result<(), PdfError> {
        let mut state = self.state.write().map_err(|_| 
            PdfError::Compression("Failed to acquire state lock".to_string()))?;

        state.compression_stats.insert(
            id.to_string(),
            CompressionStats {
                original_size: original_size as u64,
                compressed_size: compressed_size as u64,
                timestamp: Utc::parse_from_str("2025-06-02 18:42:15", "%Y-%m-%d %H:%M:%S").unwrap(),
                algorithm: CompressionAlgorithm::Zstd,
            },
        );

        Ok(())
    }
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            default_level: CompressionLevel::Default,
            stream_threshold: 1024, // Only compress streams larger than 1KB
            enable_adaptive: true,
            cache_compressed: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Stream;

    #[tokio::test]
    async fn test_compression_system_creation() {
        let writer_config = WriterConfig::default();
        let metrics = Arc::new(MetricsRegistry::new().unwrap());
        let system = CompressionSystem::new(&writer_config, metrics).await;
        assert!(system.is_ok());
    }

    #[tokio::test]
    async fn test_stream_compression() {
        let writer_config = WriterConfig::default();
        let metrics = Arc::new(MetricsRegistry::new().unwrap());
        let system = CompressionSystem::new(&writer_config, metrics).await.unwrap();
        
        let test_data = vec![0u8; 10000];
        let compressed = system.compress_with_zstd(&test_data);
        assert!(compressed.is_ok());
        assert!(compressed.unwrap().len() < test_data.len());
    }

    #[tokio::test]
    async fn test_document_compression() {
        let writer_config = WriterConfig::default();
        let metrics = Arc::new(MetricsRegistry::new().unwrap());
        let system = CompressionSystem::new(&writer_config, metrics).await.unwrap();
        
        let mut doc = Document::new();
        doc.objects.insert(
            (1, 0),
            Object::Stream(Stream::new(Dictionary::new(), vec![0u8; 10000])),
        );
        
        let result = system.compress_document(&doc).await;
        assert!(result.is_ok());
    }
}
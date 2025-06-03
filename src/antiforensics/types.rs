//! Core types for antiforensics system
//! Created: 2025-06-03 12:16:07 UTC
//! Author: kartik4091

use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
    collections::HashMap,
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Processing stages in the antiforensics pipeline
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProcessingStage {
    Initial,
    Analysis,
    Cleaning,
    Verification,
    Reporting,
    Complete,
    Failed,
}

/// Risk levels for identified issues
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RiskLevel {
    Critical,
    High,
    Medium,
    Low,
    None,
}

/// Processing metrics for tracking operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingMetrics {
    pub start_time: SystemTime,
    pub end_time: Option<SystemTime>,
    pub total_bytes: u64,
    pub processed_bytes: u64,
    pub stage: ProcessingStage,
    pub errors: Vec<String>,
    pub performance: PerformanceMetrics,
}

/// Performance metrics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub cpu_usage: f64,
    pub memory_usage: u64,
    pub io_operations: u64,
    pub processing_time: Duration,
    pub throughput: f64,
}

/// Document representation
#[derive(Debug, Clone)]
pub struct Document {
    pub id: String,
    pub path: PathBuf,
    pub size: u64,
    pub metadata: DocumentMetadata,
    pub content: Arc<RwLock<DocumentContent>>,
    pub state: DocumentState,
}

/// Document metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    pub created: SystemTime,
    pub modified: SystemTime,
    pub file_type: String,
    pub permissions: u32,
    pub owner: String,
    pub checksum: String,
    pub attributes: HashMap<String, String>,
}

/// Document content storage
#[derive(Debug, Clone)]
pub struct DocumentContent {
    pub data: Vec<u8>,
    pub chunks: Vec<ContentChunk>,
    pub encrypted: bool,
    pub compression: Option<CompressionType>,
}

/// Content chunk for efficient processing
#[derive(Debug, Clone)]
pub struct ContentChunk {
    pub offset: u64,
    pub size: u32,
    pub checksum: String,
    pub processed: bool,
}

/// Document processing state
#[derive(Debug, Clone)]
pub struct DocumentState {
    pub stage: ProcessingStage,
    pub risks: Vec<Risk>,
    pub modifications: Vec<Modification>,
    pub metrics: ProcessingMetrics,
}

/// Risk identification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Risk {
    pub id: String,
    pub level: RiskLevel,
    pub category: RiskCategory,
    pub description: String,
    pub location: Location,
    pub context: HashMap<String, String>,
    pub recommendation: String,
}

/// Risk categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RiskCategory {
    Structure,
    Content,
    Metadata,
    Encryption,
    Signature,
    External,
    Behavioral,
    Unknown,
}

/// Location in document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub offset: u64,
    pub length: u32,
    pub path: Option<String>,
    pub context: Option<String>,
}

/// Document modification record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Modification {
    pub timestamp: SystemTime,
    pub kind: ModificationType,
    pub location: Location,
    pub description: String,
    pub reversible: bool,
    pub backup: Option<String>,
}

/// Modification types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModificationType {
    Redaction,
    Encryption,
    Compression,
    Deletion,
    Transformation,
    MetadataChange,
    Custom(u32),
}

/// Compression types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompressionType {
    Gzip,
    Deflate,
    Zstd,
    Lz4,
    None,
}

impl Document {
    /// Creates a new document
    pub fn new(path: PathBuf, size: u64) -> Self {
        let id = format!("{:x}", md5::compute(path.to_string_lossy().as_bytes()));
        
        Self {
            id,
            path: path.clone(),
            size,
            metadata: DocumentMetadata::new(&path),
            content: Arc::new(RwLock::new(DocumentContent::new())),
            state: DocumentState::new(),
        }
    }

    /// Gets document size
    pub async fn size(&self) -> crate::error::Result<usize> {
        let content = self.content.read().await;
        Ok(content.data.len())
    }

    /// Checks if document is encrypted
    pub fn is_encrypted(&self) -> bool {
        self.content.blocking_read().encrypted
    }

    /// Gets document risks
    pub fn risks(&self) -> &[Risk] {
        &self.state.risks
    }
}

impl DocumentMetadata {
    /// Creates new metadata for a document
    pub fn new(path: &PathBuf) -> Self {
        Self {
            created: SystemTime::now(),
            modified: SystemTime::now(),
            file_type: Self::detect_file_type(path),
            permissions: 0o644,
            owner: whoami::username(),
            checksum: String::new(),
            attributes: HashMap::new(),
        }
    }

    /// Detects file type from path
    fn detect_file_type(path: &PathBuf) -> String {
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("unknown")
            .to_lowercase()
    }
}

impl DocumentContent {
    /// Creates new empty content
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            chunks: Vec::new(),
            encrypted: false,
            compression: None,
        }
    }
}

impl DocumentState {
    /// Creates new document state
    pub fn new() -> Self {
        Self {
            stage: ProcessingStage::Initial,
            risks: Vec::new(),
            modifications: Vec::new(),
            metrics: ProcessingMetrics::new(),
        }
    }
}

impl ProcessingMetrics {
    /// Creates new processing metrics
    pub fn new() -> Self {
        Self {
            start_time: SystemTime::now(),
            end_time: None,
            total_bytes: 0,
            processed_bytes: 0,
            stage: ProcessingStage::Initial,
            errors: Vec::new(),
            performance: PerformanceMetrics::new(),
        }
    }
}

impl PerformanceMetrics {
    /// Creates new performance metrics
    pub fn new() -> Self {
        Self {
            cpu_usage: 0.0,
            memory_usage: 0,
            io_operations: 0,
            processing_time: Duration::from_secs(0),
            throughput: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_document_creation() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_path_buf();
        let doc = Document::new(path.clone(), 0);

        assert_eq!(doc.path, path);
        assert_eq!(doc.size, 0);
        assert_eq!(doc.state.stage, ProcessingStage::Initial);
    }

    #[test]
    fn test_metadata_creation() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_path_buf();
        let metadata = DocumentMetadata::new(&path);

        assert!(!metadata.checksum.is_empty());
        assert_eq!(metadata.file_type, "tmp");
    }

    #[tokio::test]
    async fn test_document_content() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_path_buf();
        let doc = Document::new(path, 0);

        assert!(!doc.is_encrypted());
        assert_eq!(doc.size().await.unwrap(), 0);
    }

    #[test]
    fn test_risk_handling() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_path_buf();
        let doc = Document::new(path, 0);

        assert!(doc.risks().is_empty());
    }
}
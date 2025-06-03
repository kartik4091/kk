//! Content analyser for PDF document analysis
//! Author: kartik4091
//! Created: 2025-06-03 04:39:03 UTC
//! This module provides content analysis capabilities for PDF documents,
//! including text, image, and stream content analysis.

use std::{
    sync::Arc,
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};
use async_trait::async_trait;
use regex::RegexSet;
use tracing::{info, warn, error, debug, trace, instrument};

use super::AnalyserConfig;
use crate::antiforensics::{
    Document,
    PdfError,
    RiskLevel,
    ForensicArtifact,
    ArtifactType,
};

/// Content analyser implementation
pub struct ContentAnalyser {
    /// Analyser configuration
    config: Arc<AnalyserConfig>,
    /// Text pattern matcher
    text_patterns: RegexSet,
    /// Binary pattern matcher
    binary_patterns: RegexSet,
    /// Image pattern matcher
    image_patterns: RegexSet,
}

/// Content analysis context
#[derive(Debug)]
struct AnalysisContext {
    /// Memory usage in bytes
    memory_usage: usize,
    /// Analysis depth
    depth: usize,
    /// Processed objects
    processed_objects: HashSet<String>,
    /// Detected patterns
    detected_patterns: HashMap<String, Vec<PatternMatch>>,
}

/// Pattern match information
#[derive(Debug)]
struct PatternMatch {
    /// Pattern identifier
    id: String,
    /// Pattern description
    description: String,
    /// Risk level
    risk_level: RiskLevel,
    /// Match location
    offset: usize,
    /// Match length
    length: usize,
    /// Context around match
    context: Vec<u8>,
}

impl ContentAnalyser {
    /// Creates a new content analyser instance
    #[instrument(skip(config))]
    pub fn new(config: AnalyserConfig) -> Self {
        debug!("Initializing ContentAnalyser");

        Self {
            config: Arc::new(config),
            text_patterns: Self::compile_text_patterns(),
            binary_patterns: Self::compile_binary_patterns(),
            image_patterns: Self::compile_image_patterns(),
        }
    }

    /// Analyzes document content
    #[instrument(skip(self, doc), err(Display))]
    pub async fn analyze(&self, doc: &Document) -> Result<Vec<ForensicArtifact>, PdfError> {
        let mut context = AnalysisContext {
            memory_usage: 0,
            depth: 0,
            processed_objects: HashSet::new(),
            detected_patterns: HashMap::new(),
        };

        let mut artifacts = Vec::new();

        // Analyze text content
        artifacts.extend(self.analyze_text_content(doc, &mut context).await?);

        // Analyze stream content
        artifacts.extend(self.analyze_stream_content(doc, &mut context).await?);

        // Analyze image content
        artifacts.extend(self.analyze_image_content(doc, &mut context).await?);

        Ok(artifacts)
    }

    /// Analyzes text content
    async fn analyze_text_content(
        &self,
        doc: &Document,
        context: &mut AnalysisContext,
    ) -> Result<Vec<ForensicArtifact>, PdfError> {
        let mut artifacts = Vec::new();
        let text_content = doc.get_text_content()?;

        // Analyze text patterns
        for (pattern_idx, pattern) in self.text_patterns.patterns().iter().enumerate() {
            if let Some(m) = pattern.find(text_content.as_bytes()) {
                let pattern_match = PatternMatch {
                    id: format!("TEXT{:03}", pattern_idx),
                    description: "Sensitive text content detected".into(),
                    risk_level: RiskLevel::Medium,
                    offset: m.start(),
                    length: m.end() - m.start(),
                    context: self.extract_context(text_content.as_bytes(), m.start(), m.end()),
                };

                context.detected_patterns
                    .entry("text".into())
                    .or_default()
                    .push(pattern_match);
            }
        }

        // Create artifacts from detected patterns
        for matches in context.detected_patterns.get("text").unwrap_or(&Vec::new()) {
            let mut metadata = HashMap::new();
            metadata.insert("offset".into(), matches.offset.to_string());
            metadata.insert("length".into(), matches.length.to_string());
            metadata.insert("context".into(), hex::encode(&matches.context));

            artifacts.push(ForensicArtifact {
                id: uuid::Uuid::new_v4().to_string(),
                artifact_type: ArtifactType::Content,
                location: format!("text_content:{}", matches.offset),
                description: matches.description.clone(),
                risk_level: matches.risk_level,
                remediation: "Review and potentially redact sensitive text content".into(),
                metadata,
                detection_timestamp: chrono::Utc::now(),
                hash: self.calculate_hash(&matches.context),
            });
        }

        Ok(artifacts)
    }

    /// Analyzes stream content
    async fn analyze_stream_content(
        &self,
        doc: &Document,
        context: &mut AnalysisContext,
    ) -> Result<Vec<ForensicArtifact>, PdfError> {
        let mut artifacts = Vec::new();

        for stream in doc.get_streams()? {
            // Check memory limits
            let content = stream.get_decoded_content()?;
            context.memory_usage += content.len();
            if context.memory_usage > self.config.max_memory_per_analysis * 1024 * 1024 {
                warn!("Memory limit exceeded during stream analysis");
                break;
            }

            // Analyze binary patterns
            for (pattern_idx, pattern) in self.binary_patterns.patterns().iter().enumerate() {
                if let Some(m) = pattern.find(&content) {
                    let pattern_match = PatternMatch {
                        id: format!("BIN{:03}", pattern_idx),
                        description: "Suspicious binary content detected".into(),
                        risk_level: RiskLevel::High,
                        offset: m.start(),
                        length: m.end() - m.start(),
                        context: self.extract_context(&content, m.start(), m.end()),
                    };

                    context.detected_patterns
                        .entry("binary".into())
                        .or_default()
                        .push(pattern_match);
                }
            }
        }

        // Create artifacts from detected patterns
        for matches in context.detected_patterns.get("binary").unwrap_or(&Vec::new()) {
            let mut metadata = HashMap::new();
            metadata.insert("offset".into(), matches.offset.to_string());
            metadata.insert("length".into(), matches.length.to_string());
            metadata.insert("context".into(), hex::encode(&matches.context));

            artifacts.push(ForensicArtifact {
                id: uuid::Uuid::new_v4().to_string(),
                artifact_type: ArtifactType::Content,
                location: format!("stream_content:{}", matches.offset),
                description: matches.description.clone(),
                risk_level: matches.risk_level,
                remediation: "Review and remove suspicious binary content".into(),
                metadata,
                detection_timestamp: chrono::Utc::now(),
                hash: self.calculate_hash(&matches.context),
            });
        }

        Ok(artifacts)
    }

    /// Analyzes image content
    async fn analyze_image_content(
        &self,
        doc: &Document,
        context: &mut AnalysisContext,
    ) -> Result<Vec<ForensicArtifact>, PdfError> {
        let mut artifacts = Vec::new();

        for image in doc.get_images()? {
            // Check memory limits
            let content = image.get_data()?;
            context.memory_usage += content.len();
            if context.memory_usage > self.config.max_memory_per_analysis * 1024 * 1024 {
                warn!("Memory limit exceeded during image analysis");
                break;
            }

            // Analyze image patterns
            for (pattern_idx, pattern) in self.image_patterns.patterns().iter().enumerate() {
                if let Some(m) = pattern.find(&content) {
                    let pattern_match = PatternMatch {
                        id: format!("IMG{:03}", pattern_idx),
                        description: "Sensitive image metadata detected".into(),
                        risk_level: RiskLevel::Medium,
                        offset: m.start(),
                        length: m.end() - m.start(),
                        context: self.extract_context(&content, m.start(), m.end()),
                    };

                    context.detected_patterns
                        .entry("image".into())
                        .or_default()
                        .push(pattern_match);
                }
            }
        }

        // Create artifacts from detected patterns
        for matches in context.detected_patterns.get("image").unwrap_or(&Vec::new()) {
            let mut metadata = HashMap::new();
            metadata.insert("offset".into(), matches.offset.to_string());
            metadata.insert("length".into(), matches.length.to_string());
            metadata.insert("context".into(), hex::encode(&matches.context));

            artifacts.push(ForensicArtifact {
                id: uuid::Uuid::new_v4().to_string(),
                artifact_type: ArtifactType::Content,
                location: format!("image_content:{}", matches.offset),
                description: matches.description.clone(),
                risk_level: matches.risk_level,
                remediation: "Review and clean sensitive image metadata".into(),
                metadata,
                detection_timestamp: chrono::Utc::now(),
                hash: self.calculate_hash(&matches.context),
            });
        }

        Ok(artifacts)
    }

    /// Compiles text content detection patterns
    fn compile_text_patterns() -> RegexSet {
        RegexSet::new(&[
            // Sensitive information patterns
            r"(?i)password\s*[:=]",
            r"(?i)api[-_]?key\s*[:=]",
            r"(?i)secret\s*[:=]",
            // Personal information patterns
            r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b",
            r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b",  // SSN-like patterns
            r"\b\d{4}[- ]?\d{4}[- ]?\d{4}[- ]?\d{4}\b",  // Credit card patterns
            // Network information patterns
            r"\b(?:\d{1,3}\.){3}\d{1,3}\b",
            r"https?://[^\s/$.?#].[^\s]*",
        ]).expect("Failed to compile text patterns")
    }

    /// Compiles binary content detection patterns
    fn compile_binary_patterns() -> RegexSet {
        RegexSet::new(&[
            // Executable headers
            r"(?-u)\x4D\x5A",           // MZ header (DOS/PE)
            r"(?-u)\x7F\x45\x4C\x46",   // ELF header
            r"(?-u)\xCA\xFE\xBA\xBE",   // Mach-O header
            // Script patterns
            r"(?s)<script.*?>.*?</script>",
            r"javascript:",
            // Embedded file signatures
            r"(?-u)\x50\x4B\x03\x04",   // ZIP signature
            r"(?-u)\x25\x50\x44\x46",   // PDF signature
        ]).expect("Failed to compile binary patterns")
    }

    /// Compiles image content detection patterns
    fn compile_image_patterns() -> RegexSet {
        RegexSet::new(&[
            // EXIF metadata
            r"(?-u)\xFF\xE1.{2}Exif",
            // XMP metadata
            r"(?-u)<\?xmp",
            // IPTC metadata
            r"(?-u)\x1C\x02",
            // GPS information
            r"(?-u)GPS\x00",
        ]).expect("Failed to compile image patterns")
    }

    /// Extracts context around a pattern match
    fn extract_context(&self, content: &[u8], start: usize, end: usize) -> Vec<u8> {
        let context_size = 50; // Number of bytes before and after match
        let start_idx = start.saturating_sub(context_size);
        let end_idx = (end + context_size).min(content.len());
        content[start_idx..end_idx].to_vec()
    }

    /// Calculates hash of pattern context
    fn calculate_hash(&self, data: &[u8]) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    #[test]
    async fn test_text_pattern_detection() {
        let analyser = ContentAnalyser::new(AnalyserConfig::default());
        let mut doc = Document::new();
        doc.add_text_content("password: secret123");
        
        let mut context = AnalysisContext {
            memory_usage: 0,
            depth: 0,
            processed_objects: HashSet::new(),
            detected_patterns: HashMap::new(),
        };

        let artifacts = analyser.analyze_text_content(&doc, &mut context).await.unwrap();
        assert!(!artifacts.is_empty());
        assert_eq!(artifacts[0].risk_level, RiskLevel::Medium);
    }

    #[test]
    async fn test_binary_pattern_detection() {
        let analyser = ContentAnalyser::new(AnalyserConfig::default());
        let mut doc = Document::new();
        doc.add_stream("test", vec![0x4D, 0x5A, 0x90, 0x00]); // MZ header
        
        let mut context = AnalysisContext {
            memory_usage: 0,
            depth: 0,
            processed_objects: HashSet::new(),
            detected_patterns: HashMap::new(),
        };

        let artifacts = analyser.analyze_stream_content(&doc, &mut context).await.unwrap();
        assert!(!artifacts.is_empty());
        assert_eq!(artifacts[0].risk_level, RiskLevel::High);
    }

    #[test]
    async fn test_image_pattern_detection() {
        let analyser = ContentAnalyser::new(AnalyserConfig::default());
        let mut doc = Document::new();
        let mut image_data = vec![0xFF, 0xE1, 0x00, 0x10];
        image_data.extend(b"Exif\0\0");
        doc.add_image("test", image_data);
        
        let mut context = AnalysisContext {
            memory_usage: 0,
            depth: 0,
            processed_objects: HashSet::new(),
            detected_patterns: HashMap::new(),
        };

        let artifacts = analyser.analyze_image_content(&doc, &mut context).await.unwrap();
        assert!(!artifacts.is_empty());
        assert_eq!(artifacts[0].risk_level, RiskLevel::Medium);
    }

    #[test]
    async fn test_memory_limit() {
        let config = AnalyserConfig {
            max_memory_per_analysis: 1, // 1MB limit
            ..Default::default()
        };
        let analyser = ContentAnalyser::new(config);
        let mut doc = Document::new();
        
        // Add large stream
        let large_stream = vec![0; 2 * 1024 * 1024]; // 2MB
        doc.add_stream("large", large_stream);
        
        let mut context = AnalysisContext {
            memory_usage: 0,
            depth: 0,
            processed_objects: HashSet::new(),
            detected_patterns: HashMap::new(),
        };

        let artifacts = analyser.analyze_stream_content(&doc, &mut context).await.unwrap();
        assert!(artifacts.is_empty());
    }
      }

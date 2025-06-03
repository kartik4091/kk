//! Stream scanner for PDF document content analysis
//! Author: kartik4091
//! Created: 2025-06-03 04:24:57 UTC
//! This module provides stream content analysis capabilities for PDF documents,
//! detecting potentially malicious or sensitive content in stream objects.

use std::{
    sync::Arc,
    collections::HashMap,
    time::{Duration, Instant},
};
use async_trait::async_trait;
use regex::bytes::RegexSet;
use tracing::{info, warn, error, debug, trace, instrument};

use super::{ScannerConfig, ScanContext};
use crate::antiforensics::{
    Document,
    PdfError,
    RiskLevel,
    ForensicArtifact,
    ArtifactType,
};

/// Stream scanner for content analysis
pub struct StreamScanner {
    /// Scanner configuration
    config: Arc<ScannerConfig>,
    /// Compiled pattern set for binary content
    binary_patterns: RegexSet,
    /// Compiled pattern set for text content
    text_patterns: RegexSet,
    /// Compiled pattern set for image metadata
    image_patterns: RegexSet,
    /// JavaScript detection patterns
    js_patterns: RegexSet,
}

/// Stream content type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamType {
    /// Plain text content
    Text,
    /// Image content
    Image,
    /// JavaScript content
    JavaScript,
    /// Form XObject
    Form,
    /// Binary content
    Binary,
    /// Unknown content type
    Unknown,
}

/// Stream analysis result
#[derive(Debug)]
struct StreamAnalysis {
    /// Stream identifier
    id: String,
    /// Content type
    content_type: StreamType,
    /// Detected patterns
    patterns: Vec<PatternMatch>,
    /// Stream size in bytes
    size: usize,
    /// Analysis duration
    duration: Duration,
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

impl StreamScanner {
    /// Creates a new stream scanner instance
    #[instrument(skip(config))]
    pub fn new(config: ScannerConfig) -> Self {
        debug!("Initializing StreamScanner");

        Self {
            config: Arc::new(config),
            binary_patterns: Self::compile_binary_patterns(),
            text_patterns: Self::compile_text_patterns(),
            image_patterns: Self::compile_image_patterns(),
            js_patterns: Self::compile_js_patterns(),
        }
    }

    /// Compiles binary content detection patterns
    fn compile_binary_patterns() -> RegexSet {
        RegexSet::new(&[
            // Executable headers
            r"(?-u)\x4D\x5A",           // MZ header (DOS/PE)
            r"(?-u)\x7F\x45\x4C\x46",   // ELF header
            r"(?-u)\xCA\xFE\xBA\xBE",   // Mach-O header
            // Compressed data
            r"(?-u)\x50\x4B\x03\x04",   // ZIP signature
            r"(?-u)\x1F\x8B\x08",       // GZIP signature
            // Embedded files
            r"(?-u)%PDF-\d\.\d",        // Embedded PDF
            r"(?-u)\xFF\xD8\xFF",       // JPEG signature
        ]).expect("Failed to compile binary patterns")
    }

    /// Compiles text content detection patterns
    fn compile_text_patterns() -> RegexSet {
        RegexSet::new(&[
            // Sensitive information
            r"(?i)password\s*[:=]",
            r"(?i)api[-_]?key\s*[:=]",
            r"(?i)secret\s*[:=]",
            // URLs and endpoints
            r"https?://[^\s/$.?#].[^\s]*",
            r"(?i)localhost:[0-9]+",
            // Email addresses
            r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b",
            // IP addresses
            r"\b(?:\d{1,3}\.){3}\d{1,3}\b",
        ]).expect("Failed to compile text patterns")
    }

    /// Compiles image metadata detection patterns
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

    /// Compiles JavaScript detection patterns
    fn compile_js_patterns() -> RegexSet {
        RegexSet::new(&[
            // JavaScript markers
            r"/JavaScript\s*>>\s*",
            r"(?i)function\s+[a-z0-9_]+\s*\(",
            // Potentially malicious functions
            r"(?i)eval\s*\(",
            r"(?i)unescape\s*\(",
            // DOM manipulation
            r"(?i)document\.",
            r"(?i)window\.",
            // Network operations
            r"(?i)xmlhttp",
            r"(?i)fetch\s*\(",
        ]).expect("Failed to compile JavaScript patterns")
    }

    /// Scans a stream object for forensic artifacts
    #[instrument(skip(self, stream, context), err(Display))]
    pub async fn scan_stream(
        &self,
        stream: &Stream,
        context: &mut ScanContext,
    ) -> Result<Vec<ForensicArtifact>, PdfError> {
        let start_time = Instant::now();
        let stream_type = self.determine_stream_type(stream)?;
        let content = stream.get_decoded_content()?;

        // Update memory usage tracking
        context.memory_usage += content.len();
        context.check_memory_limit(&self.config)?;

        let analysis = self.analyze_stream(
            stream.get_id()?,
            &content,
            stream_type,
            start_time.elapsed(),
        )?;

        Ok(self.create_artifacts(stream, &analysis))
    }

    /// Determines the type of stream content
    fn determine_stream_type(&self, stream: &Stream) -> Result<StreamType, PdfError> {
        let dict = stream.get_dictionary()?;
        
        if dict.has_key("JavaScript") {
            Ok(StreamType::JavaScript)
        } else if dict.has_key("Subtype") {
            match dict.get_name("Subtype")?.as_deref() {
                Some("Image") => Ok(StreamType::Image),
                Some("Form") => Ok(StreamType::Form),
                Some("Text") => Ok(StreamType::Text),
                _ => self.detect_stream_type(&stream.get_decoded_content()?),
            }
        } else {
            self.detect_stream_type(&stream.get_decoded_content()?)
        }
    }

    /// Detects stream type from content
    fn detect_stream_type(&self, content: &[u8]) -> Result<StreamType, PdfError> {
        // Check for text content (high proportion of ASCII characters)
        let ascii_ratio = content.iter()
            .filter(|&&b| b.is_ascii_graphic() || b.is_ascii_whitespace())
            .count() as f64 / content.len() as f64;

        if ascii_ratio > 0.8 {
            if self.js_patterns.is_match(content) {
                Ok(StreamType::JavaScript)
            } else {
                Ok(StreamType::Text)
            }
        } else if content.starts_with(b"%PDF") {
            Ok(StreamType::Binary)
        } else if content.starts_with(b"\xFF\xD8\xFF") || // JPEG
                  content.starts_with(b"\x89PNG") ||      // PNG
                  content.starts_with(b"GIF8") {          // GIF
            Ok(StreamType::Image)
        } else {
            Ok(StreamType::Binary)
        }
    }

    /// Analyzes stream content for patterns
    fn analyze_stream(
        &self,
        id: String,
        content: &[u8],
        content_type: StreamType,
        duration: Duration,
    ) -> Result<StreamAnalysis, PdfError> {
        let patterns = match content_type {
            StreamType::Text => self.analyze_text_content(content)?,
            StreamType::Image => self.analyze_image_content(content)?,
            StreamType::JavaScript => self.analyze_javascript_content(content)?,
            StreamType::Binary => self.analyze_binary_content(content)?,
            _ => Vec::new(),
        };

        Ok(StreamAnalysis {
            id,
            content_type,
            patterns,
            size: content.len(),
            duration,
        })
    }

    /// Analyzes text content
    fn analyze_text_content(&self, content: &[u8]) -> Result<Vec<PatternMatch>, PdfError> {
        let mut matches = Vec::new();
        
        for (idx, pattern) in self.text_patterns.patterns().iter().enumerate() {
            if let Some(m) = pattern.find(content) {
                matches.push(PatternMatch {
                    id: format!("TEXT{:03}", idx),
                    description: "Sensitive text content detected".into(),
                    risk_level: RiskLevel::Medium,
                    offset: m.start(),
                    length: m.end() - m.start(),
                    context: self.extract_context(content, m.start(), m.end()),
                });
            }
        }

        Ok(matches)
    }

    /// Analyzes image content
    fn analyze_image_content(&self, content: &[u8]) -> Result<Vec<PatternMatch>, PdfError> {
        let mut matches = Vec::new();
        
        for (idx, pattern) in self.image_patterns.patterns().iter().enumerate() {
            if let Some(m) = pattern.find(content) {
                matches.push(PatternMatch {
                    id: format!("IMG{:03}", idx),
                    description: "Sensitive image metadata detected".into(),
                    risk_level: RiskLevel::Medium,
                    offset: m.start(),
                    length: m.end() - m.start(),
                    context: self.extract_context(content, m.start(), m.end()),
                });
            }
        }

        Ok(matches)
    }

    /// Analyzes JavaScript content
    fn analyze_javascript_content(&self, content: &[u8]) -> Result<Vec<PatternMatch>, PdfError> {
        let mut matches = Vec::new();
        
        for (idx, pattern) in self.js_patterns.patterns().iter().enumerate() {
            if let Some(m) = pattern.find(content) {
                matches.push(PatternMatch {
                    id: format!("JS{:03}", idx),
                    description: "Potentially malicious JavaScript detected".into(),
                    risk_level: RiskLevel::High,
                    offset: m.start(),
                    length: m.end() - m.start(),
                    context: self.extract_context(content, m.start(), m.end()),
                });
            }
        }

        Ok(matches)
    }

    /// Analyzes binary content
    fn analyze_binary_content(&self, content: &[u8]) -> Result<Vec<PatternMatch>, PdfError> {
        let mut matches = Vec::new();
        
        for (idx, pattern) in self.binary_patterns.patterns().iter().enumerate() {
            if let Some(m) = pattern.find(content) {
                matches.push(PatternMatch {
                    id: format!("BIN{:03}", idx),
                    description: "Suspicious binary content detected".into(),
                    risk_level: RiskLevel::Critical,
                    offset: m.start(),
                    length: m.end() - m.start(),
                    context: self.extract_context(content, m.start(), m.end()),
                });
            }
        }

        Ok(matches)
    }

    /// Extracts context around a pattern match
    fn extract_context(&self, content: &[u8], start: usize, end: usize) -> Vec<u8> {
        let context_size = 50; // Number of bytes before and after match
        let start_idx = start.saturating_sub(context_size);
        let end_idx = (end + context_size).min(content.len());
        content[start_idx..end_idx].to_vec()
    }

    /// Creates forensic artifacts from stream analysis
    fn create_artifacts(&self, stream: &Stream, analysis: &StreamAnalysis) -> Vec<ForensicArtifact> {
        let mut artifacts = Vec::new();

        for pattern in &analysis.patterns {
            let mut metadata = HashMap::new();
            metadata.insert("stream_type".into(), format!("{:?}", analysis.content_type));
            metadata.insert("stream_size".into(), analysis.size.to_string());
            metadata.insert("offset".into(), pattern.offset.to_string());
            metadata.insert("context".into(), hex::encode(&pattern.context));

            artifacts.push(ForensicArtifact {
                id: uuid::Uuid::new_v4().to_string(),
                artifact_type: match analysis.content_type {
                    StreamType::JavaScript => ArtifactType::JavaScript,
                    StreamType::Binary => ArtifactType::Binary,
                    _ => ArtifactType::Content,
                },
                location: format!("stream_{}", analysis.id),
                description: pattern.description.clone(),
                risk_level: pattern.risk_level,
                remediation: self.generate_remediation(pattern, analysis.content_type),
                metadata,
                detection_timestamp: chrono::Utc::now(),
                hash: self.calculate_hash(&pattern.context),
            });
        }

        artifacts
    }

    /// Generates remediation advice
    fn generate_remediation(&self, pattern: &PatternMatch, stream_type: StreamType) -> String {
        match stream_type {
            StreamType::JavaScript => format!(
                "Review and potentially remove JavaScript code at offset {}. \
                Consider using static content instead.", 
                pattern.offset
            ),
            StreamType::Binary => format!(
                "Remove embedded binary content at offset {}. \
                Ensure no executable code is present.", 
                pattern.offset
            ),
            StreamType::Image => format!(
                "Clean image metadata at offset {}. \
                Remove any sensitive EXIF/XMP data.", 
                pattern.offset
            ),
            _ => format!(
                "Review and clean sensitive content at offset {}. \
                Consider content redaction.", 
                pattern.offset
            ),
        }
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
    async fn test_stream_type_detection() {
        let scanner = StreamScanner::new(ScannerConfig::default());
        
        assert_eq!(
            scanner.detect_stream_type(b"function test() { }").unwrap(),
            StreamType::JavaScript
        );
        
        assert_eq!(
            scanner.detect_stream_type(b"\xFF\xD8\xFF\xE0").unwrap(),
            StreamType::Image
        );
        
        assert_eq!(
            scanner.detect_stream_type(b"This is plain text").unwrap(),
            StreamType::Text
        );
    }

    #[test]
    async fn test_pattern_matching() {
        let scanner = StreamScanner::new(ScannerConfig::default());
        
        let js_content = b"function eval(malicious_code) { }";
        let matches = scanner.analyze_javascript_content(js_content).unwrap();
        assert!(!matches.is_empty());
        
        let text_content = b"password: secret123";
        let matches = scanner.analyze_text_content(text_content).unwrap();
        assert!(!matches.is_empty());
    }

    #[test]
    async fn test_context_extraction() {
        let scanner = StreamScanner::new(ScannerConfig::default());
        let content = b"prefix password: secret123 suffix";
        
        let context = scanner.extract_context(content, 7, 23);
        assert!(context.len() <= 100); // Context size + match length
        assert!(context.starts_with(b"prefix"));
        assert!(context.ends_with(b"suffix"));
    }

    #[test]
    async fn test_binary_detection() {
        let scanner = StreamScanner::new(ScannerConfig::default());
        
        let binary_content = b"MZ\x90\x00\x03\x00\x00\x00";
        let matches = scanner.analyze_binary_content(binary_content).unwrap();
        assert!(!matches.is_empty());
        assert_eq!(matches[0].risk_level, RiskLevel::Critical);
    }
}
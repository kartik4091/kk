//! Content cleaner for PDF document sanitization
//! Author: kartik4091
//! Created: 2025-06-03 04:30:42 UTC
//! This module provides content-specific cleaning capabilities
//! for removing or sanitizing sensitive content in PDF documents.

use std::{
    sync::Arc,
    collections::HashMap,
    time::{Duration, Instant},
};
use async_trait::async_trait;
use tracing::{info, warn, error, debug, trace, instrument};

use super::CleaningConfig;
use crate::antiforensics::{
    Document,
    PdfError,
    RiskLevel,
    ForensicArtifact,
    ArtifactType,
};

/// Result of content cleaning operation
#[derive(Debug)]
pub struct ContentCleaningStats {
    /// Number of artifacts successfully cleaned
    pub cleaned: usize,
    /// List of failed cleanings
    pub failed: Vec<super::FailedCleaning>,
}

/// Content cleaner implementation
pub struct ContentCleaner {
    /// Cleaner configuration
    config: Arc<CleaningConfig>,
    /// Cleaning strategies
    strategies: Vec<Box<dyn CleaningStrategy>>,
}

/// Interface for content cleaning strategies
#[async_trait]
trait CleaningStrategy: Send + Sync {
    /// Attempts to clean content
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError>;

    /// Returns strategy name
    fn name(&self) -> &'static str;
}

/// Strategy for redacting text content
struct TextRedactionStrategy;

/// Strategy for removing embedded content
struct EmbeddedContentStrategy;

/// Strategy for sanitizing image content
struct ImageSanitizationStrategy;

/// Strategy for cleaning stream content
struct StreamSanitizationStrategy;

impl ContentCleaner {
    /// Creates a new content cleaner instance
    #[instrument(skip(config))]
    pub fn new(config: CleaningConfig) -> Self {
        debug!("Initializing ContentCleaner");

        let strategies: Vec<Box<dyn CleaningStrategy>> = vec![
            Box::new(TextRedactionStrategy),
            Box::new(EmbeddedContentStrategy),
            Box::new(ImageSanitizationStrategy),
            Box::new(StreamSanitizationStrategy),
        ];

        Self {
            config: Arc::new(config),
            strategies,
        }
    }

    /// Cleans content artifacts from a document
    #[instrument(skip(self, doc, artifacts), err(Display))]
    pub async fn clean(
        &self,
        doc: &mut Document,
        artifacts: &[ForensicArtifact],
    ) -> Result<ContentCleaningStats, PdfError> {
        let mut stats = ContentCleaningStats {
            cleaned: 0,
            failed: Vec::new(),
        };

        for artifact in artifacts {
            let mut cleaned = false;
            let mut last_error = None;

            // Try each strategy until one succeeds
            for strategy in &self.strategies {
                match strategy.clean(doc, artifact).await {
                    Ok(true) => {
                        cleaned = true;
                        debug!(
                            "Successfully cleaned artifact {} using strategy {}",
                            artifact.id,
                            strategy.name()
                        );
                        break;
                    }
                    Ok(false) => continue,
                    Err(e) => {
                        warn!(
                            "Strategy {} failed to clean artifact {}: {}",
                            strategy.name(),
                            artifact.id,
                            e
                        );
                        last_error = Some(e);
                    }
                }
            }

            if cleaned {
                stats.cleaned += 1;
            } else {
                stats.failed.push(super::FailedCleaning {
                    artifact: artifact.clone(),
                    error: last_error
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "No suitable cleaning strategy found".into()),
                    strategy: "content".into(),
                });
            }
        }

        Ok(stats)
    }
}

#[async_trait]
impl CleaningStrategy for TextRedactionStrategy {
    #[instrument(skip(self, doc, artifact), err(Display))]
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError> {
        if !matches!(artifact.artifact_type, ArtifactType::Content) {
            return Ok(false);
        }

        let content = doc.get_text_content()?;
        if let Some(location) = content.find(&artifact.location) {
            // Create redaction annotation
            doc.add_redaction_annotation(
                location,
                artifact.location.len(),
                "Sensitive content redacted",
            )?;

            // Apply redactions
            doc.apply_redactions()?;
            
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn name(&self) -> &'static str {
        "text_redaction"
    }
}

#[async_trait]
impl CleaningStrategy for EmbeddedContentStrategy {
    #[instrument(skip(self, doc, artifact), err(Display))]
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError> {
        if !matches!(artifact.artifact_type, ArtifactType::Content) {
            return Ok(false);
        }

        // Check if the artifact is embedded content
        if let Some(embedded_obj) = doc.get_embedded_object(&artifact.location)? {
            // Remove the embedded object
            doc.remove_embedded_object(&artifact.location)?;
            
            // Verify removal
            if doc.get_embedded_object(&artifact.location)?.is_none() {
                Ok(true)
            } else {
                Err(PdfError::Cleaner("Failed to remove embedded content".into()))
            }
        } else {
            Ok(false)
        }
    }

    fn name(&self) -> &'static str {
        "embedded_content"
    }
}

#[async_trait]
impl CleaningStrategy for ImageSanitizationStrategy {
    #[instrument(skip(self, doc, artifact), err(Display))]
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError> {
        if !matches!(artifact.artifact_type, ArtifactType::Content) {
            return Ok(false);
        }

        if let Some(image) = doc.get_image(&artifact.location)? {
            // Remove metadata from image
            let mut sanitized_image = image.clone();
            sanitized_image.remove_metadata()?;

            // Replace original image
            doc.replace_image(&artifact.location, sanitized_image)?;
            
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn name(&self) -> &'static str {
        "image_sanitization"
    }
}

#[async_trait]
impl CleaningStrategy for StreamSanitizationStrategy {
    #[instrument(skip(self, doc, artifact), err(Display))]
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError> {
        if !matches!(artifact.artifact_type, ArtifactType::Content) {
            return Ok(false);
        }

        if let Some(stream) = doc.get_stream(&artifact.location)? {
            // Get stream content
            let mut content = stream.get_decoded_content()?;
            
            // Remove sensitive data
            if let Some(offset) = artifact.metadata.get("offset").and_then(|s| s.parse().ok()) {
                if let Some(length) = artifact.metadata.get("length").and_then(|s| s.parse().ok()) {
                    // Replace sensitive data with null bytes
                    for i in offset..offset + length {
                        if i < content.len() {
                            content[i] = 0;
                        }
                    }

                    // Update stream content
                    stream.set_content(&content)?;
                    
                    Ok(true)
                } else {
                    Ok(false)
                }
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    fn name(&self) -> &'static str {
        "stream_sanitization"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    #[test]
    async fn test_text_redaction() {
        let mut doc = Document::new();
        doc.add_text_content("This is sensitive information");
        
        let artifact = ForensicArtifact {
            artifact_type: ArtifactType::Content,
            location: "sensitive".into(),
            ..Default::default()
        };

        let strategy = TextRedactionStrategy;
        assert!(strategy.clean(&mut doc, &artifact).await.unwrap());
        assert!(!doc.get_text_content().unwrap().contains("sensitive"));
    }

    #[test]
    async fn test_embedded_content_removal() {
        let mut doc = Document::new();
        doc.embed_object("test.exe", &[1, 2, 3, 4]);
        
        let artifact = ForensicArtifact {
            artifact_type: ArtifactType::Content,
            location: "test.exe".into(),
            ..Default::default()
        };

        let strategy = EmbeddedContentStrategy;
        assert!(strategy.clean(&mut doc, &artifact).await.unwrap());
        assert!(doc.get_embedded_object("test.exe").unwrap().is_none());
    }

    #[test]
    async fn test_image_sanitization() {
        let mut doc = Document::new();
        let mut image = Image::new();
        image.add_metadata("GPS", "12.345,67.890");
        doc.add_image("test.jpg", image);
        
        let artifact = ForensicArtifact {
            artifact_type: ArtifactType::Content,
            location: "test.jpg".into(),
            ..Default::default()
        };

        let strategy = ImageSanitizationStrategy;
        assert!(strategy.clean(&mut doc, &artifact).await.unwrap());
        assert!(doc.get_image("test.jpg").unwrap().get_metadata().is_empty());
    }

    #[test]
    async fn test_stream_sanitization() {
        let mut doc = Document::new();
        let stream = Stream::new(vec![1, 2, 3, 4, 5]);
        doc.add_stream("test", stream);
        
        let mut metadata = HashMap::new();
        metadata.insert("offset".into(), "1".into());
        metadata.insert("length".into(), "2".into());
        
        let artifact = ForensicArtifact {
            artifact_type: ArtifactType::Content,
            location: "test".into(),
            metadata,
            ..Default::default()
        };

        let strategy = StreamSanitizationStrategy;
        assert!(strategy.clean(&mut doc, &artifact).await.unwrap());
        
        let cleaned_stream = doc.get_stream("test").unwrap();
        let content = cleaned_stream.get_decoded_content().unwrap();
        assert_eq!(content[1..3], vec![0, 0]);
    }

    #[test]
    async fn test_content_cleaner() {
        let cleaner = ContentCleaner::new(CleaningConfig::default());
        let mut doc = Document::new();
        
        let artifacts = vec![
            ForensicArtifact {
                artifact_type: ArtifactType::Content,
                location: "sensitive".into(),
                ..Default::default()
            },
            ForensicArtifact {
                artifact_type: ArtifactType::Content,
                location: "test.exe".into(),
                ..Default::default()
            },
        ];

        let stats = cleaner.clean(&mut doc, &artifacts).await.unwrap();
        assert!(stats.cleaned > 0);
    }
}
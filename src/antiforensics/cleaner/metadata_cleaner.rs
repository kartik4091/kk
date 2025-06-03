//! Metadata cleaner for PDF document sanitization
//! Author: kartik4091
//! Created: 2025-06-03 04:32:07 UTC
//! This module provides metadata cleaning capabilities
//! for removing or sanitizing document metadata.

use std::{
    sync::Arc,
    collections::HashMap,
    time::{Duration, Instant},
};
use async_trait::async_trait;
use tracing::{info, warn, error, debug, trace, instrument};
use chrono::Utc;

use super::CleaningConfig;
use crate::antiforensics::{
    Document,
    PdfError,
    RiskLevel,
    ForensicArtifact,
    ArtifactType,
};

/// Result of metadata cleaning operation
#[derive(Debug)]
pub struct MetadataCleaningStats {
    /// Number of artifacts successfully cleaned
    pub cleaned: usize,
    /// List of failed cleanings
    pub failed: Vec<super::FailedCleaning>,
}

/// Metadata cleaner implementation
pub struct MetadataCleaner {
    /// Cleaner configuration
    config: Arc<CleaningConfig>,
    /// Cleaning strategies
    strategies: Vec<Box<dyn MetadataStrategy>>,
}

/// Interface for metadata cleaning strategies
#[async_trait]
trait MetadataStrategy: Send + Sync {
    /// Attempts to clean metadata
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError>;

    /// Returns strategy name
    fn name(&self) -> &'static str;
}

/// Strategy for cleaning document information
struct DocumentInfoStrategy;

/// Strategy for cleaning XMP metadata
struct XmpMetadataStrategy;

/// Strategy for cleaning embedded metadata
struct EmbeddedMetadataStrategy;

/// Strategy for cleaning custom metadata
struct CustomMetadataStrategy;

impl MetadataCleaner {
    /// Creates a new metadata cleaner instance
    #[instrument(skip(config))]
    pub fn new(config: CleaningConfig) -> Self {
        debug!("Initializing MetadataCleaner");

        let strategies: Vec<Box<dyn MetadataStrategy>> = vec![
            Box::new(DocumentInfoStrategy),
            Box::new(XmpMetadataStrategy),
            Box::new(EmbeddedMetadataStrategy),
            Box::new(CustomMetadataStrategy),
        ];

        Self {
            config: Arc::new(config),
            strategies,
        }
    }

    /// Cleans metadata artifacts from a document
    #[instrument(skip(self, doc, artifacts), err(Display))]
    pub async fn clean(
        &self,
        doc: &mut Document,
        artifacts: &[ForensicArtifact],
    ) -> Result<MetadataCleaningStats, PdfError> {
        let mut stats = MetadataCleaningStats {
            cleaned: 0,
            failed: Vec::new(),
        };

        for artifact in artifacts {
            if !matches!(artifact.artifact_type, ArtifactType::Metadata) {
                continue;
            }

            let mut cleaned = false;
            let mut last_error = None;

            // Try each strategy until one succeeds
            for strategy in &self.strategies {
                match strategy.clean(doc, artifact).await {
                    Ok(true) => {
                        cleaned = true;
                        debug!(
                            "Successfully cleaned metadata artifact {} using strategy {}",
                            artifact.id,
                            strategy.name()
                        );
                        break;
                    }
                    Ok(false) => continue,
                    Err(e) => {
                        warn!(
                            "Strategy {} failed to clean metadata artifact {}: {}",
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
                        .unwrap_or_else(|| "No suitable metadata cleaning strategy found".into()),
                    strategy: "metadata".into(),
                });
            }
        }

        Ok(stats)
    }
}

#[async_trait]
impl MetadataStrategy for DocumentInfoStrategy {
    #[instrument(skip(self, doc, artifact), err(Display))]
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError> {
        let info = doc.get_info_dict()?;
        
        if let Some(key) = info.get_key(&artifact.location) {
            // Remove or sanitize the metadata field
            match key {
                "Title" | "Subject" | "Keywords" => {
                    // Preserve structure but remove content
                    info.set_string(&artifact.location, "")?;
                }
                "Author" | "Creator" | "Producer" => {
                    // Replace with generic values
                    info.set_string(&artifact.location, "Sanitized")?;
                }
                "CreationDate" | "ModDate" => {
                    // Update to current time
                    info.set_date(&artifact.location, &Utc::now())?;
                }
                _ => {
                    // Remove unknown fields
                    info.remove(&artifact.location)?;
                }
            }

            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn name(&self) -> &'static str {
        "document_info"
    }
}

#[async_trait]
impl MetadataStrategy for XmpMetadataStrategy {
    #[instrument(skip(self, doc, artifact), err(Display))]
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError> {
        if let Some(xmp) = doc.get_xmp_metadata()? {
            if xmp.has_property(&artifact.location) {
                match artifact.location.as_str() {
                    "xmp:CreateDate" | "xmp:ModifyDate" => {
                        // Update timestamps
                        xmp.set_date(&artifact.location, &Utc::now())?;
                    }
                    "dc:creator" | "pdf:Producer" => {
                        // Replace with generic values
                        xmp.set_array(&artifact.location, vec!["Sanitized"])?;
                    }
                    "xmp:MetadataDate" => {
                        // Update metadata timestamp
                        xmp.set_date(&artifact.location, &Utc::now())?;
                    }
                    _ => {
                        // Remove other properties
                        xmp.remove_property(&artifact.location)?;
                    }
                }

                doc.update_xmp_metadata(&xmp)?;
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    fn name(&self) -> &'static str {
        "xmp_metadata"
    }
}

#[async_trait]
impl MetadataStrategy for EmbeddedMetadataStrategy {
    #[instrument(skip(self, doc, artifact), err(Display))]
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError> {
        // Handle embedded metadata in images, forms, etc.
        if let Some(obj) = doc.get_object(&artifact.location)? {
            if let Some(metadata) = obj.get_metadata()? {
                // Remove all metadata
                obj.remove_metadata()?;
                
                // Update the object
                doc.update_object(&artifact.location, obj)?;
                
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    fn name(&self) -> &'static str {
        "embedded_metadata"
    }
}

#[async_trait]
impl MetadataStrategy for CustomMetadataStrategy {
    #[instrument(skip(self, doc, artifact), err(Display))]
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError> {
        // Handle custom metadata fields
        if let Some(metadata_dict) = doc.get_metadata_dict()? {
            if metadata_dict.has_key(&artifact.location) {
                metadata_dict.remove(&artifact.location)?;
                doc.update_metadata_dict(metadata_dict)?;
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    fn name(&self) -> &'static str {
        "custom_metadata"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    #[test]
    async fn test_document_info_cleaning() {
        let mut doc = Document::new();
        doc.set_info_string("Author", "John Doe").unwrap();
        
        let artifact = ForensicArtifact {
            artifact_type: ArtifactType::Metadata,
            location: "Author".into(),
            ..Default::default()
        };

        let strategy = DocumentInfoStrategy;
        assert!(strategy.clean(&mut doc, &artifact).await.unwrap());
        assert_eq!(doc.get_info_string("Author").unwrap(), "Sanitized");
    }

    #[test]
    async fn test_xmp_metadata_cleaning() {
        let mut doc = Document::new();
        let mut xmp = XmpMetadata::new();
        xmp.set_creator("John Doe");
        doc.set_xmp_metadata(&xmp).unwrap();
        
        let artifact = ForensicArtifact {
            artifact_type: ArtifactType::Metadata,
            location: "dc:creator".into(),
            ..Default::default()
        };

        let strategy = XmpMetadataStrategy;
        assert!(strategy.clean(&mut doc, &artifact).await.unwrap());
        assert_eq!(
            doc.get_xmp_metadata().unwrap().get_creator(),
            vec!["Sanitized"]
        );
    }

    #[test]
    async fn test_embedded_metadata_cleaning() {
        let mut doc = Document::new();
        let mut obj = PdfObject::new();
        obj.set_metadata("GPS", "12.345,67.890");
        doc.add_object("image1", obj);
        
        let artifact = ForensicArtifact {
            artifact_type: ArtifactType::Metadata,
            location: "image1".into(),
            ..Default::default()
        };

        let strategy = EmbeddedMetadataStrategy;
        assert!(strategy.clean(&mut doc, &artifact).await.unwrap());
        assert!(doc.get_object("image1")
            .unwrap()
            .get_metadata()
            .unwrap()
            .is_empty());
    }

    #[test]
    async fn test_custom_metadata_cleaning() {
        let mut doc = Document::new();
        let mut metadata = Dictionary::new();
        metadata.set("CustomField", "sensitive data");
        doc.set_metadata_dict(&metadata).unwrap();
        
        let artifact = ForensicArtifact {
            artifact_type: ArtifactType::Metadata,
            location: "CustomField".into(),
            ..Default::default()
        };

        let strategy = CustomMetadataStrategy;
        assert!(strategy.clean(&mut doc, &artifact).await.unwrap());
        assert!(!doc.get_metadata_dict()
            .unwrap()
            .has_key("CustomField"));
    }

    #[test]
    async fn test_metadata_cleaner() {
        let cleaner = MetadataCleaner::new(CleaningConfig::default());
        let mut doc = Document::new();
        doc.set_info_string("Author", "John Doe").unwrap();
        
        let artifacts = vec![
            ForensicArtifact {
                artifact_type: ArtifactType::Metadata,
                location: "Author".into(),
                ..Default::default()
            },
        ];

        let stats = cleaner.clean(&mut doc, &artifacts).await.unwrap();
        assert_eq!(stats.cleaned, 1);
        assert!(stats.failed.is_empty());
    }
}
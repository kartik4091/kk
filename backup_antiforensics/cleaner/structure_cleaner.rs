//! Structure cleaner for PDF document sanitization
//! Author: kartik4091
//! Created: 2025-06-03 04:35:17 UTC
//! This module provides structural cleaning capabilities
//! for removing or sanitizing document structure elements.

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

/// Result of structure cleaning operation
#[derive(Debug)]
pub struct StructureCleaningStats {
    /// Number of artifacts successfully cleaned
    pub cleaned: usize,
    /// List of failed cleanings
    pub failed: Vec<super::FailedCleaning>,
}

/// Structure cleaner implementation
pub struct StructureCleaner {
    /// Cleaner configuration
    config: Arc<CleaningConfig>,
    /// Cleaning strategies
    strategies: Vec<Box<dyn StructureStrategy>>,
}

/// Interface for structure cleaning strategies
#[async_trait]
trait StructureStrategy: Send + Sync {
    /// Attempts to clean structure
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError>;

    /// Returns strategy name
    fn name(&self) -> &'static str;
}

/// Strategy for cleaning named destinations
struct NamedDestinationStrategy;

/// Strategy for cleaning document outline
struct OutlineStrategy;

/// Strategy for cleaning page tree
struct PageTreeStrategy;

/// Strategy for cleaning optional content
struct OptionalContentStrategy;

impl StructureCleaner {
    /// Creates a new structure cleaner instance
    #[instrument(skip(config))]
    pub fn new(config: CleaningConfig) -> Self {
        debug!("Initializing StructureCleaner");

        let strategies: Vec<Box<dyn StructureStrategy>> = vec![
            Box::new(NamedDestinationStrategy),
            Box::new(OutlineStrategy),
            Box::new(PageTreeStrategy),
            Box::new(OptionalContentStrategy),
        ];

        Self {
            config: Arc::new(config),
            strategies,
        }
    }

    /// Cleans structure artifacts from a document
    #[instrument(skip(self, doc, artifacts), err(Display))]
    pub async fn clean(
        &self,
        doc: &mut Document,
        artifacts: &[ForensicArtifact],
    ) -> Result<StructureCleaningStats, PdfError> {
        let mut stats = StructureCleaningStats {
            cleaned: 0,
            failed: Vec::new(),
        };

        for artifact in artifacts {
            if !matches!(artifact.artifact_type, ArtifactType::Structure) {
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
                            "Successfully cleaned structure artifact {} using strategy {}",
                            artifact.id,
                            strategy.name()
                        );
                        break;
                    }
                    Ok(false) => continue,
                    Err(e) => {
                        warn!(
                            "Strategy {} failed to clean structure artifact {}: {}",
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
                        .unwrap_or_else(|| "No suitable structure cleaning strategy found".into()),
                    strategy: "structure".into(),
                });
            }
        }

        Ok(stats)
    }
}

#[async_trait]
impl StructureStrategy for NamedDestinationStrategy {
    #[instrument(skip(self, doc, artifact), err(Display))]
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError> {
        // Handle named destinations in the Names tree
        if let Some(names) = doc.get_names()? {
            if let Some(dests) = names.get_destinations()? {
                if dests.remove(&artifact.location)? {
                    // Update the Names tree
                    doc.update_names(&names)?;
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn name(&self) -> &'static str {
        "named_destination"
    }
}

#[async_trait]
impl StructureStrategy for OutlineStrategy {
    #[instrument(skip(self, doc, artifact), err(Display))]
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError> {
        // Handle document outline (bookmarks)
        if let Some(outline) = doc.get_outline_mut()? {
            let mut cleaned = false;
            
            // Recursively clean outline items
            fn clean_item(
                item: &mut OutlineItem,
                location: &str,
            ) -> Result<bool, PdfError> {
                // Check if this item matches
                if item.get_destination()? == location {
                    item.remove_destination()?;
                    return Ok(true);
                }

                // Check children
                for child in item.get_children_mut() {
                    if clean_item(child, location)? {
                        return Ok(true);
                    }
                }

                Ok(false)
            }

            // Clean from root items
            for item in outline.get_items_mut() {
                if clean_item(item, &artifact.location)? {
                    cleaned = true;
                    break;
                }
            }

            if cleaned {
                doc.update_outline(&outline)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn name(&self) -> &'static str {
        "outline"
    }
}

#[async_trait]
impl StructureStrategy for PageTreeStrategy {
    #[instrument(skip(self, doc, artifact), err(Display))]
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError> {
        // Handle page tree structure
        if let Some(page_tree) = doc.get_page_tree_mut()? {
            // Check if artifact refers to a page node
            if let Some(node) = page_tree.find_node(&artifact.location)? {
                match artifact.metadata.get("action").map(String::as_str) {
                    Some("remove") => {
                        // Remove the page and its descendants
                        page_tree.remove_node(&artifact.location)?;
                        doc.update_page_tree(&page_tree)?;
                        Ok(true)
                    }
                    Some("sanitize") => {
                        // Remove sensitive attributes but keep structure
                        node.remove_attributes()?;
                        doc.update_page_tree(&page_tree)?;
                        Ok(true)
                    }
                    _ => Ok(false),
                }
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    fn name(&self) -> &'static str {
        "page_tree"
    }
}

#[async_trait]
impl StructureStrategy for OptionalContentStrategy {
    #[instrument(skip(self, doc, artifact), err(Display))]
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError> {
        // Handle optional content (layers)
        if let Some(oc_properties) = doc.get_optional_content_properties_mut()? {
            if let Some(group) = oc_properties.find_group(&artifact.location)? {
                match artifact.metadata.get("action").map(String::as_str) {
                    Some("remove") => {
                        // Remove the optional content group
                        oc_properties.remove_group(&artifact.location)?;
                        doc.update_optional_content_properties(&oc_properties)?;
                        Ok(true)
                    }
                    Some("disable") => {
                        // Disable the optional content group
                        group.set_visible(false)?;
                        doc.update_optional_content_properties(&oc_properties)?;
                        Ok(true)
                    }
                    _ => Ok(false),
                }
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    fn name(&self) -> &'static str {
        "optional_content"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    #[test]
    async fn test_named_destination_cleaning() {
        let mut doc = Document::new();
        doc.add_named_destination("test", Destination::new_xyz(1, 0.0, 0.0, 1.0)).unwrap();
        
        let artifact = ForensicArtifact {
            artifact_type: ArtifactType::Structure,
            location: "test".into(),
            ..Default::default()
        };

        let strategy = NamedDestinationStrategy;
        assert!(strategy.clean(&mut doc, &artifact).await.unwrap());
        assert!(doc.get_names()
            .unwrap()
            .get_destinations()
            .unwrap()
            .is_empty());
    }

    #[test]
    async fn test_outline_cleaning() {
        let mut doc = Document::new();
        let mut outline = Outline::new();
        outline.add_item(OutlineItem::new("Test", "test_dest")).unwrap();
        doc.set_outline(&outline).unwrap();
        
        let artifact = ForensicArtifact {
            artifact_type: ArtifactType::Structure,
            location: "test_dest".into(),
            ..Default::default()
        };

        let strategy = OutlineStrategy;
        assert!(strategy.clean(&mut doc, &artifact).await.unwrap());
        assert!(doc.get_outline()
            .unwrap()
            .get_items()
            .next()
            .unwrap()
            .get_destination()
            .unwrap()
            .is_empty());
    }

    #[test]
    async fn test_page_tree_cleaning() {
        let mut doc = Document::new();
        let page_ref = doc.add_page().unwrap();
        
        let mut metadata = HashMap::new();
        metadata.insert("action".into(), "remove".into());
        
        let artifact = ForensicArtifact {
            artifact_type: ArtifactType::Structure,
            location: page_ref,
            metadata,
            ..Default::default()
        };

        let strategy = PageTreeStrategy;
        assert!(strategy.clean(&mut doc, &artifact).await.unwrap());
        assert_eq!(doc.get_page_count().unwrap(), 0);
    }

    #[test]
    async fn test_optional_content_cleaning() {
        let mut doc = Document::new();
        doc.add_optional_content_group("test", "Test Layer").unwrap();
        
        let mut metadata = HashMap::new();
        metadata.insert("action".into(), "disable".into());
        
        let artifact = ForensicArtifact {
            artifact_type: ArtifactType::Structure,
            location: "test".into(),
            metadata,
            ..Default::default()
        };

        let strategy = OptionalContentStrategy;
        assert!(strategy.clean(&mut doc, &artifact).await.unwrap());
        assert!(!doc.get_optional_content_properties()
            .unwrap()
            .find_group("test")
            .unwrap()
            .is_visible()
            .unwrap());
    }

    #[test]
    async fn test_structure_cleaner() {
        let cleaner = StructureCleaner::new(CleaningConfig::default());
        let mut doc = Document::new();
        doc.add_named_destination("test", Destination::new_xyz(1, 0.0, 0.0, 1.0)).unwrap();
        
        let artifacts = vec![
            ForensicArtifact {
                artifact_type: ArtifactType::Structure,
                location: "test".into(),
                ..Default::default()
            },
        ];

        let stats = cleaner.clean(&mut doc, &artifacts).await.unwrap();
        assert_eq!(stats.cleaned, 1);
        assert!(stats.failed.is_empty());
    }
}
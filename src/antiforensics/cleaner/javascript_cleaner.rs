//! JavaScript cleaner for PDF document sanitization
//! Author: kartik4091
//! Created: 2025-06-03 04:33:38 UTC
//! This module provides JavaScript cleaning capabilities
//! for removing or neutralizing JavaScript code in PDF documents.

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

/// Result of JavaScript cleaning operation
#[derive(Debug)]
pub struct JavaScriptCleaningStats {
    /// Number of artifacts successfully cleaned
    pub cleaned: usize,
    /// List of failed cleanings
    pub failed: Vec<super::FailedCleaning>,
}

/// JavaScript cleaner implementation
pub struct JavaScriptCleaner {
    /// Cleaner configuration
    config: Arc<CleaningConfig>,
    /// Cleaning strategies
    strategies: Vec<Box<dyn JavaScriptStrategy>>,
}

/// Interface for JavaScript cleaning strategies
#[async_trait]
trait JavaScriptStrategy: Send + Sync {
    /// Attempts to clean JavaScript
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError>;

    /// Returns strategy name
    fn name(&self) -> &'static str;
}

/// Strategy for cleaning document-level JavaScript
struct DocumentJavaScriptStrategy;

/// Strategy for cleaning action JavaScript
struct ActionJavaScriptStrategy;

/// Strategy for cleaning form JavaScript
struct FormJavaScriptStrategy;

/// Strategy for cleaning annotation JavaScript
struct AnnotationJavaScriptStrategy;

impl JavaScriptCleaner {
    /// Creates a new JavaScript cleaner instance
    #[instrument(skip(config))]
    pub fn new(config: CleaningConfig) -> Self {
        debug!("Initializing JavaScriptCleaner");

        let strategies: Vec<Box<dyn JavaScriptStrategy>> = vec![
            Box::new(DocumentJavaScriptStrategy),
            Box::new(ActionJavaScriptStrategy),
            Box::new(FormJavaScriptStrategy),
            Box::new(AnnotationJavaScriptStrategy),
        ];

        Self {
            config: Arc::new(config),
            strategies,
        }
    }

    /// Cleans JavaScript artifacts from a document
    #[instrument(skip(self, doc, artifacts), err(Display))]
    pub async fn clean(
        &self,
        doc: &mut Document,
        artifacts: &[ForensicArtifact],
    ) -> Result<JavaScriptCleaningStats, PdfError> {
        let mut stats = JavaScriptCleaningStats {
            cleaned: 0,
            failed: Vec::new(),
        };

        for artifact in artifacts {
            if !matches!(artifact.artifact_type, ArtifactType::JavaScript) {
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
                            "Successfully cleaned JavaScript artifact {} using strategy {}",
                            artifact.id,
                            strategy.name()
                        );
                        break;
                    }
                    Ok(false) => continue,
                    Err(e) => {
                        warn!(
                            "Strategy {} failed to clean JavaScript artifact {}: {}",
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
                        .unwrap_or_else(|| "No suitable JavaScript cleaning strategy found".into()),
                    strategy: "javascript".into(),
                });
            }
        }

        Ok(stats)
    }
}

#[async_trait]
impl JavaScriptStrategy for DocumentJavaScriptStrategy {
    #[instrument(skip(self, doc, artifact), err(Display))]
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError> {
        // Handle document-level JavaScript (in Names tree)
        if let Some(names) = doc.get_names()? {
            if let Some(js_names) = names.get_javascript()? {
                if js_names.remove(&artifact.location)? {
                    // Update the Names tree
                    doc.update_names(&names)?;
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn name(&self) -> &'static str {
        "document_javascript"
    }
}

#[async_trait]
impl JavaScriptStrategy for ActionJavaScriptStrategy {
    #[instrument(skip(self, doc, artifact), err(Display))]
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError> {
        // Handle JavaScript actions (OpenAction, AA)
        if let Some(catalog) = doc.get_catalog_mut()? {
            // Check OpenAction
            if let Some(open_action) = catalog.get_open_action()? {
                if open_action.get_type()? == "JavaScript" &&
                   open_action.get_script()? == artifact.location {
                    catalog.remove_open_action()?;
                    return Ok(true);
                }
            }

            // Check Additional Actions
            if let Some(aa) = catalog.get_additional_actions()? {
                for action_type in ["WC", "WS", "DS", "WP", "DP"] {
                    if let Some(action) = aa.get_action(action_type)? {
                        if action.get_type()? == "JavaScript" &&
                           action.get_script()? == artifact.location {
                            aa.remove_action(action_type)?;
                            catalog.update_additional_actions(&aa)?;
                            return Ok(true);
                        }
                    }
                }
            }
        }
        Ok(false)
    }

    fn name(&self) -> &'static str {
        "action_javascript"
    }
}

#[async_trait]
impl JavaScriptStrategy for FormJavaScriptStrategy {
    #[instrument(skip(self, doc, artifact), err(Display))]
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError> {
        // Handle form field JavaScript
        if let Some(acro_form) = doc.get_acro_form_mut()? {
            for field in acro_form.get_fields_mut() {
                // Check field actions
                if let Some(actions) = field.get_actions()? {
                    let mut modified = false;
                    
                    for action_type in ["K", "F", "V"] {
                        if let Some(action) = actions.get_action(action_type)? {
                            if action.get_type()? == "JavaScript" &&
                               action.get_script()? == artifact.location {
                                actions.remove_action(action_type)?;
                                modified = true;
                            }
                        }
                    }

                    if modified {
                        field.update_actions(&actions)?;
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    fn name(&self) -> &'static str {
        "form_javascript"
    }
}

#[async_trait]
impl JavaScriptStrategy for AnnotationJavaScriptStrategy {
    #[instrument(skip(self, doc, artifact), err(Display))]
    async fn clean(
        &self,
        doc: &mut Document,
        artifact: &ForensicArtifact,
    ) -> Result<bool, PdfError> {
        // Handle annotation JavaScript
        for page in doc.get_pages_mut() {
            for annot in page.get_annotations_mut() {
                // Check annotation actions
                if let Some(action) = annot.get_action()? {
                    if action.get_type()? == "JavaScript" &&
                       action.get_script()? == artifact.location {
                        annot.remove_action()?;
                        return Ok(true);
                    }
                }

                // Check additional actions
                if let Some(aa) = annot.get_additional_actions()? {
                    let mut modified = false;
                    
                    for action_type in ["E", "X", "D", "U", "Fo", "Bl"] {
                        if let Some(action) = aa.get_action(action_type)? {
                            if action.get_type()? == "JavaScript" &&
                               action.get_script()? == artifact.location {
                                aa.remove_action(action_type)?;
                                modified = true;
                            }
                        }
                    }

                    if modified {
                        annot.update_additional_actions(&aa)?;
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    fn name(&self) -> &'static str {
        "annotation_javascript"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    #[test]
    async fn test_document_javascript_cleaning() {
        let mut doc = Document::new();
        doc.add_javascript("test", "alert('test')").unwrap();
        
        let artifact = ForensicArtifact {
            artifact_type: ArtifactType::JavaScript,
            location: "test".into(),
            ..Default::default()
        };

        let strategy = DocumentJavaScriptStrategy;
        assert!(strategy.clean(&mut doc, &artifact).await.unwrap());
        assert!(doc.get_names().unwrap().get_javascript().unwrap().is_empty());
    }

    #[test]
    async fn test_action_javascript_cleaning() {
        let mut doc = Document::new();
        let action = JavaScriptAction::new("alert('test')");
        doc.set_open_action(action).unwrap();
        
        let artifact = ForensicArtifact {
            artifact_type: ArtifactType::JavaScript,
            location: "alert('test')".into(),
            ..Default::default()
        };

        let strategy = ActionJavaScriptStrategy;
        assert!(strategy.clean(&mut doc, &artifact).await.unwrap());
        assert!(doc.get_catalog().unwrap().get_open_action().unwrap().is_none());
    }

    #[test]
    async fn test_form_javascript_cleaning() {
        let mut doc = Document::new();
        let mut field = FormField::new("test");
        field.add_calculate_action("alert('test')").unwrap();
        doc.add_form_field(field).unwrap();
        
        let artifact = ForensicArtifact {
            artifact_type: ArtifactType::JavaScript,
            location: "alert('test')".into(),
            ..Default::default()
        };

        let strategy = FormJavaScriptStrategy;
        assert!(strategy.clean(&mut doc, &artifact).await.unwrap());
        assert!(doc.get_acro_form()
            .unwrap()
            .get_fields()
            .next()
            .unwrap()
            .get_actions()
            .unwrap()
            .is_empty());
    }

    #[test]
    async fn test_annotation_javascript_cleaning() {
        let mut doc = Document::new();
        let mut annot = Annotation::new();
        annot.set_action(JavaScriptAction::new("alert('test')")).unwrap();
        doc.add_annotation(annot).unwrap();
        
        let artifact = ForensicArtifact {
            artifact_type: ArtifactType::JavaScript,
            location: "alert('test')".into(),
            ..Default::default()
        };

        let strategy = AnnotationJavaScriptStrategy;
        assert!(strategy.clean(&mut doc, &artifact).await.unwrap());
        assert!(doc.get_pages()
            .next()
            .unwrap()
            .get_annotations()
            .next()
            .unwrap()
            .get_action()
            .unwrap()
            .is_none());
    }

    #[test]
    async fn test_javascript_cleaner() {
        let cleaner = JavaScriptCleaner::new(CleaningConfig::default());
        let mut doc = Document::new();
        doc.add_javascript("test", "alert('test')").unwrap();
        
        let artifacts = vec![
            ForensicArtifact {
                artifact_type: ArtifactType::JavaScript,
                location: "test".into(),
                ..Default::default()
            },
        ];

        let stats = cleaner.clean(&mut doc, &artifacts).await.unwrap();
        assert_eq!(stats.cleaned, 1);
        assert!(stats.failed.is_empty());
    }
}
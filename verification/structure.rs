use crate::{PdfError, VerificationError, VerificationWarning, ErrorSeverity};
use chrono::{DateTime, Utc};
use lopdf::{Document, Object, ObjectId, Dictionary};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};

pub struct StructureVerifier {
    state: Arc<RwLock<StructureState>>,
    config: StructureConfig,
}

struct StructureState {
    verifications_performed: u64,
    last_verification: Option<DateTime<Utc>>,
    active_verifications: u32,
    verification_cache: HashMap<String, StructureResult>,
}

#[derive(Clone)]
struct StructureConfig {
    max_tree_depth: usize,
    max_indirect_refs: usize,
    required_root_entries: HashSet<String>,
    required_page_entries: HashSet<String>,
    validate_references: bool,
    validate_streams: bool,
}

#[derive(Debug, Clone)]
pub struct StructureResult {
    pub errors: Vec<VerificationError>,
    pub warnings: Vec<VerificationWarning>,
    pub rules_checked: usize,
    pub tree_depth: usize,
    pub ref_count: usize,
}

impl StructureVerifier {
    pub async fn new() -> Result<Self, PdfError> {
        Ok(Self {
            state: Arc::new(RwLock::new(StructureState {
                verifications_performed: 0,
                last_verification: None,
                active_verifications: 0,
                verification_cache: HashMap::new(),
            })),
            config: StructureConfig::default(),
        })
    }

    pub async fn verify(&self, doc: &Document) -> Result<StructureResult, PdfError> {
        let start_time = std::time::Instant::now();
        let current_time = Utc::parse_from_str("2025-06-02 18:56:57", "%Y-%m-%d %H:%M:%S")
            .map_err(|_| PdfError::Verification("Invalid current time".to_string()))?;

        // Update state
        {
            let mut state = self.state.write().map_err(|_| 
                PdfError::Verification("Failed to acquire state lock".to_string()))?;
            state.active_verifications += 1;
        }

        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut rules_checked = 0;

        // Verify document structure
        self.verify_root_structure(doc, &mut errors, &mut warnings, &mut rules_checked)?;
        self.verify_page_tree(doc, &mut errors, &mut warnings, &mut rules_checked)?;
        self.verify_references(doc, &mut errors, &mut warnings, &mut rules_checked)?;
        self.verify_streams(doc, &mut errors, &mut warnings, &mut rules_checked)?;

        // Calculate structure metrics
        let tree_depth = self.calculate_tree_depth(doc)?;
        let ref_count = self.count_references(doc)?;

        // Create result
        let result = StructureResult {
            errors,
            warnings,
            rules_checked,
            tree_depth,
            ref_count,
        };

        // Update state
        {
            let mut state = self.state.write().map_err(|_| 
                PdfError::Verification("Failed to acquire state lock".to_string()))?;
            state.active_verifications -= 1;
            state.verifications_performed += 1;
            state.last_verification = Some(current_time);
            state.verification_cache.insert(doc.get_id().unwrap_or_default(), result.clone());
        }

        Ok(result)
    }

    fn verify_root_structure(
        &self,
        doc: &Document,
        errors: &mut Vec<VerificationError>,
        warnings: &mut Vec<VerificationWarning>,
        rules_checked: &mut usize,
    ) -> Result<(), PdfError> {
        *rules_checked += 1;

        // Verify catalog
        if let Some(catalog_id) = doc.catalog {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&catalog_id) {
                // Check required entries
                for entry in &self.config.required_root_entries {
                    if !dict.has(entry) {
                        errors.push(VerificationError {
                            code: "MISSING_ROOT_ENTRY".to_string(),
                            message: format!("Required root entry '{}' is missing", entry),
                            location: Some(catalog_id),
                            severity: ErrorSeverity::Critical,
                            details: HashMap::new(),
                        });
                    }
                }

                // Check type
                if dict.get("Type") != Ok(&Object::Name("Catalog".to_string())) {
                    errors.push(VerificationError {
                        code: "INVALID_CATALOG_TYPE".to_string(),
                        message: "Invalid or missing Catalog type".to_string(),
                        location: Some(catalog_id),
                        severity: ErrorSeverity::Critical,
                        details: HashMap::new(),
                    });
                }
            }
        } else {
            errors.push(VerificationError {
                code: "MISSING_CATALOG".to_string(),
                message: "Document catalog is missing".to_string(),
                location: None,
                severity: ErrorSeverity::Critical,
                details: HashMap::new(),
            });
        }

        Ok(())
    }

    fn verify_page_tree(
        &self,
        doc: &Document,
        errors: &mut Vec<VerificationError>,
        warnings: &mut Vec<VerificationWarning>,
        rules_checked: &mut usize,
    ) -> Result<(), PdfError> {
        *rules_checked += 1;

        let mut visited = HashSet::new();
        if let Some(pages_id) = self.find_pages_root(doc)? {
            self.verify_page_node(doc, pages_id, 0, &mut visited, errors, warnings)?;
        } else {
            errors.push(VerificationError {
                code: "MISSING_PAGES_ROOT".to_string(),
                message: "Document is missing Pages tree root".to_string(),
                location: None,
                severity: ErrorSeverity::Critical,
                details: HashMap::new(),
            });
        }

        Ok(())
    }

    fn verify_page_node(
        &self,
        doc: &Document,
        node_id: ObjectId,
        depth: usize,
        visited: &mut HashSet<ObjectId>,
        errors: &mut Vec<VerificationError>,
        warnings: &mut Vec<VerificationWarning>,
    ) -> Result<(), PdfError> {
        // Check for circular references
        if !visited.insert(node_id) {
            errors.push(VerificationError {
                code: "CIRCULAR_REFERENCE".to_string(),
                message: "Circular reference detected in page tree".to_string(),
                location: Some(node_id),
                severity: ErrorSeverity::Critical,
                details: HashMap::new(),
            });
            return Ok(());
        }

        // Check depth limit
        if depth > self.config.max_tree_depth {
            errors.push(VerificationError {
                code: "EXCESSIVE_TREE_DEPTH".to_string(),
                message: format!("Page tree depth exceeds maximum of {}", self.config.max_tree_depth),
                location: Some(node_id),
                severity: ErrorSeverity::Major,
                details: HashMap::new(),
            });
            return Ok(());
        }

        if let Some(Object::Dictionary(dict)) = doc.objects.get(&node_id) {
            // Verify node type
            match dict.get("Type") {
                Ok(Object::Name(name)) => {
                    match name.as_str() {
                        "Pages" => self.verify_pages_node(doc, dict, node_id, depth, visited, errors, warnings)?,
                        "Page" => self.verify_page_node_entries(dict, node_id, errors, warnings)?,
                        _ => {
                            errors.push(VerificationError {
                                code: "INVALID_NODE_TYPE".to_string(),
                                message: format!("Invalid page tree node type: {}", name),
                                location: Some(node_id),
                                severity: ErrorSeverity::Major,
                                details: HashMap::new(),
                            });
                        }
                    }
                }
                _ => {
                    errors.push(VerificationError {
                        code: "MISSING_NODE_TYPE".to_string(),
                        message: "Missing page tree node type".to_string(),
                        location: Some(node_id),
                        severity: ErrorSeverity::Major,
                        details: HashMap::new(),
                    });
                }
            }
        }

        Ok(())
    }

    fn verify_references(
        &self,
        doc: &Document,
        errors: &mut Vec<VerificationError>,
        warnings: &mut Vec<VerificationWarning>,
        rules_checked: &mut usize,
    ) -> Result<(), PdfError> {
        if !self.config.validate_references {
            return Ok(());
        }

        *rules_checked += 1;
        let mut ref_count = 0;

        for (id, obj) in &doc.objects {
            self.check_object_references(doc, *id, obj, &mut ref_count, errors, warnings)?;
        }

        if ref_count > self.config.max_indirect_refs {
            warnings.push(VerificationWarning {
                code: "EXCESSIVE_REFERENCES".to_string(),
                message: format!("Document contains {} indirect references", ref_count),
                location: None,
                recommendation: "Consider optimizing document structure".to_string(),
            });
        }

        Ok(())
    }

    fn verify_streams(
        &self,
        doc: &Document,
        errors: &mut Vec<VerificationError>,
        warnings: &mut Vec<VerificationWarning>,
        rules_checked: &mut usize,
    ) -> Result<(), PdfError> {
        if !self.config.validate_streams {
            return Ok(());
        }

        *rules_checked += 1;

        for (id, obj) in &doc.objects {
            if let Object::Stream(stream) = obj {
                // Verify stream dictionary
                if !stream.dict.has("Length") {
                    errors.push(VerificationError {
                        code: "MISSING_STREAM_LENGTH".to_string(),
                        message: "Stream is missing Length entry".to_string(),
                        location: Some(*id),
                        severity: ErrorSeverity::Major,
                        details: HashMap::new(),
                    });
                }

                // Verify filters
                if let Ok(Object::Array(filters)) = stream.dict.get("Filter") {
                    for filter in filters {
                        if let Object::Name(name) = filter {
                            if !self.is_valid_filter(name) {
                                errors.push(VerificationError {
                                    code: "INVALID_STREAM_FILTER".to_string(),
                                    message: format!("Invalid stream filter: {}", name),
                                    location: Some(*id),
                                    severity: ErrorSeverity::Major,
                                    details: HashMap::new(),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn calculate_tree_depth(&self, doc: &Document) -> Result<usize, PdfError> {
        if let Some(pages_id) = self.find_pages_root(doc)? {
            let mut visited = HashSet::new();
            Ok(self.calculate_node_depth(doc, pages_id, &mut visited)?)
        } else {
            Ok(0)
        }
    }

    fn count_references(&self, doc: &Document) -> Result<usize, PdfError> {
        let mut count = 0;
        let mut visited = HashSet::new();

        for obj in doc.objects.values() {
            self.count_object_references(obj, &mut count, &mut visited)?;
        }

        Ok(count)
    }

    // Helper methods...
    fn find_pages_root(&self, doc: &Document) -> Result<Option<ObjectId>, PdfError> {
        if let Some(catalog_id) = doc.catalog {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&catalog_id) {
                if let Ok(Object::Reference(pages_id)) = dict.get("Pages") {
                    return Ok(Some(*pages_id));
                }
            }
        }
        Ok(None)
    }

    fn is_valid_filter(&self, filter_name: &str) -> bool {
        matches!(
            filter_name,
            "ASCIIHexDecode" | "ASCII85Decode" | "LZWDecode" | "FlateDecode" |
            "RunLengthDecode" | "CCITTFaxDecode" | "JBIG2Decode" | "DCTDecode" |
            "JPXDecode" | "Crypt"
        )
    }
}

impl Default for StructureConfig {
    fn default() -> Self {
        let mut required_root_entries = HashSet::new();
        required_root_entries.insert("Type".to_string());
        required_root_entries.insert("Pages".to_string());

        let mut required_page_entries = HashSet::new();
        required_page_entries.insert("Type".to_string());
        required_page_entries.insert("Parent".to_string());

        Self {
            max_tree_depth: 32,
            max_indirect_refs: 1_000_000,
            required_root_entries,
            required_page_entries,
            validate_references: true,
            validate_streams: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_structure_verifier_creation() {
        let verifier = StructureVerifier::new().await;
        assert!(verifier.is_ok());
    }

    #[tokio::test]
    async fn test_basic_structure_verification() {
        let verifier = StructureVerifier::new().await.unwrap();
        let doc = Document::new();
        let result = verifier.verify(&doc).await;
        assert!(result.is_ok());
        
        let verification = result.unwrap();
        assert!(!verification.errors.is_empty()); // Should have catalog error
    }

    #[tokio::test]
    async fn test_page_tree_verification() {
        let verifier = StructureVerifier::new().await.unwrap();
        let mut doc = Document::new();
        
        // Create basic page tree
        let pages_dict = Dictionary::from_iter(vec![
            ("Type", Object::Name("Pages".to_string())),
            ("Count", Object::Integer(0)),
            ("Kids", Object::Array(vec![])),
        ]);
        let pages_id = doc.add_object(pages_dict);
        
        // Set catalog
        let catalog_dict = Dictionary::from_iter(vec![
            ("Type", Object::Name("Catalog".to_string())),
            ("Pages", Object::Reference(pages_id)),
        ]);
        doc.catalog = Some(doc.add_object(catalog_dict));
        
        let result = verifier.verify(&doc).await;
        assert!(result.is_ok());
    }
}
//! Object scanner for PDF document structure analysis
//! Author: kartik4091
//! Created: 2025-06-03 04:27:05 UTC
//! This module provides object-level analysis capabilities for PDF documents,
//! including dictionary, array, and indirect object analysis.

use std::{
    sync::Arc,
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};
use async_trait::async_trait;
use tracing::{info, warn, error, debug, trace, instrument};

use super::{ScannerConfig, ScanContext};
use crate::antiforensics::{
    Document,
    PdfError,
    RiskLevel,
    ForensicArtifact,
    ArtifactType,
};

/// Scanner for PDF objects
pub struct ObjectScanner {
    /// Scanner configuration
    config: Arc<ScannerConfig>,
    /// Set of known safe object types
    safe_types: HashSet<String>,
    /// Map of known risky keys to their risk levels
    risky_keys: HashMap<String, RiskLevel>,
}

/// Object analysis result
#[derive(Debug)]
struct ObjectAnalysis {
    /// Object identifier
    id: String,
    /// Object type
    object_type: ObjectType,
    /// Analysis findings
    findings: Vec<Finding>,
    /// Analysis duration
    duration: Duration,
    /// Object depth in document structure
    depth: usize,
}

/// PDF object types
#[derive(Debug, Clone, PartialEq, Eq)]
enum ObjectType {
    Dictionary,
    Array,
    Stream,
    String,
    Number,
    Boolean,
    Null,
    Name,
    Reference,
}

/// Analysis finding
#[derive(Debug)]
struct Finding {
    /// Finding identifier
    id: String,
    /// Description
    description: String,
    /// Risk level
    risk_level: RiskLevel,
    /// Path to the finding in the object
    path: String,
    /// Related value
    value: String,
}

impl ObjectScanner {
    /// Creates a new object scanner instance
    #[instrument(skip(config))]
    pub fn new(config: ScannerConfig) -> Self {
        debug!("Initializing ObjectScanner");

        let mut scanner = Self {
            config: Arc::new(config),
            safe_types: HashSet::new(),
            risky_keys: HashMap::new(),
        };

        scanner.initialize_safe_types();
        scanner.initialize_risky_keys();

        scanner
    }

    /// Initializes set of known safe object types
    fn initialize_safe_types(&mut self) {
        let safe_types = [
            "Font", "XObject", "ExtGState", "ColorSpace",
            "Pattern", "Shading", "Properties", "Metadata",
        ];

        self.safe_types.extend(safe_types.iter().map(|&s| s.to_string()));
    }

    /// Initializes map of risky keys
    fn initialize_risky_keys(&mut self) {
        self.risky_keys.insert("JavaScript".to_string(), RiskLevel::Critical);
        self.risky_keys.insert("JS".to_string(), RiskLevel::Critical);
        self.risky_keys.insert("Launch".to_string(), RiskLevel::Critical);
        self.risky_keys.insert("SubmitForm".to_string(), RiskLevel::High);
        self.risky_keys.insert("ImportData".to_string(), RiskLevel::High);
        self.risky_keys.insert("RichMedia".to_string(), RiskLevel::High);
        self.risky_keys.insert("OpenAction".to_string(), RiskLevel::High);
        self.risky_keys.insert("AA".to_string(), RiskLevel::High);
        self.risky_keys.insert("URI".to_string(), RiskLevel::Medium);
        self.risky_keys.insert("GoTo".to_string(), RiskLevel::Medium);
    }

    /// Scans a PDF object for forensic artifacts
    #[instrument(skip(self, obj, context), err(Display))]
    pub async fn scan_object(
        &self,
        obj: &PdfObject,
        context: &mut ScanContext,
    ) -> Result<Vec<ForensicArtifact>, PdfError> {
        let start_time = Instant::now();

        // Check recursion depth
        context.check_recursion_limit(&self.config)?;

        // Check if object has been processed
        let obj_id = obj.get_id()?;
        if !context.processed_objects.insert(obj_id.clone()) {
            return Ok(Vec::new());
        }

        let analysis = self.analyze_object(obj, context, start_time.elapsed())?;
        Ok(self.create_artifacts(&analysis))
    }

    /// Analyzes a PDF object
    fn analyze_object(
        &self,
        obj: &PdfObject,
        context: &mut ScanContext,
        duration: Duration,
    ) -> Result<ObjectAnalysis, PdfError> {
        let object_type = self.determine_object_type(obj);
        let mut findings = Vec::new();

        match object_type {
            ObjectType::Dictionary => {
                self.analyze_dictionary(obj.as_dictionary()?, &mut findings, context)?;
            }
            ObjectType::Array => {
                self.analyze_array(obj.as_array()?, &mut findings, context)?;
            }
            ObjectType::Stream => {
                self.analyze_stream_dict(obj.as_stream()?.get_dictionary()?, &mut findings)?;
            }
            ObjectType::String => {
                self.analyze_string(obj.as_string()?, &mut findings)?;
            }
            _ => {}
        }

        Ok(ObjectAnalysis {
            id: obj.get_id()?,
            object_type,
            findings,
            duration,
            depth: context.depth,
        })
    }

    /// Determines object type
    fn determine_object_type(&self, obj: &PdfObject) -> ObjectType {
        match obj {
            PdfObject::Dictionary(_) => ObjectType::Dictionary,
            PdfObject::Array(_) => ObjectType::Array,
            PdfObject::Stream(_) => ObjectType::Stream,
            PdfObject::String(_) => ObjectType::String,
            PdfObject::Number(_) => ObjectType::Number,
            PdfObject::Boolean(_) => ObjectType::Boolean,
            PdfObject::Null => ObjectType::Null,
            PdfObject::Name(_) => ObjectType::Name,
            PdfObject::Reference(_) => ObjectType::Reference,
        }
    }

    /// Analyzes a dictionary object
    fn analyze_dictionary(
        &self,
        dict: &Dictionary,
        findings: &mut Vec<Finding>,
        context: &mut ScanContext,
    ) -> Result<(), PdfError> {
        for (key, value) in dict.iter() {
            // Check for risky keys
            if let Some(risk_level) = self.risky_keys.get(key) {
                findings.push(Finding {
                    id: uuid::Uuid::new_v4().to_string(),
                    description: format!("Potentially risky key '{}' found", key),
                    risk_level: *risk_level,
                    path: format!("/{}", key),
                    value: value.to_string(),
                });
            }

            // Recursively analyze nested objects
            match value {
                PdfObject::Dictionary(d) => {
                    context.depth += 1;
                    self.analyze_dictionary(d, findings, context)?;
                    context.depth -= 1;
                }
                PdfObject::Array(a) => {
                    context.depth += 1;
                    self.analyze_array(a, findings, context)?;
                    context.depth -= 1;
                }
                _ => self.analyze_value(value, &format!("/{}", key), findings)?,
            }
        }

        Ok(())
    }

    /// Analyzes an array object
    fn analyze_array(
        &self,
        array: &Vec<PdfObject>,
        findings: &mut Vec<Finding>,
        context: &mut ScanContext,
    ) -> Result<(), PdfError> {
        for (index, value) in array.iter().enumerate() {
            match value {
                PdfObject::Dictionary(d) => {
                    context.depth += 1;
                    self.analyze_dictionary(d, findings, context)?;
                    context.depth -= 1;
                }
                PdfObject::Array(a) => {
                    context.depth += 1;
                    self.analyze_array(a, findings, context)?;
                    context.depth -= 1;
                }
                _ => self.analyze_value(value, &format!("/[{}]", index), findings)?,
            }
        }

        Ok(())
    }

    /// Analyzes a stream dictionary
    fn analyze_stream_dict(
        &self,
        dict: &Dictionary,
        findings: &mut Vec<Finding>,
    ) -> Result<(), PdfError> {
        // Check for encryption or compression methods
        if let Some(filter) = dict.get("Filter") {
            match filter {
                PdfObject::Name(name) if name == "Crypt" => {
                    findings.push(Finding {
                        id: uuid::Uuid::new_v4().to_string(),
                        description: "Encrypted stream content detected".into(),
                        risk_level: RiskLevel::High,
                        path: "/Filter".into(),
                        value: name.clone(),
                    });
                }
                PdfObject::Array(filters) => {
                    for filter in filters {
                        if let PdfObject::Name(name) = filter {
                            if name == "Crypt" {
                                findings.push(Finding {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    description: "Encrypted stream content detected".into(),
                                    risk_level: RiskLevel::High,
                                    path: "/Filter".into(),
                                    value: name.clone(),
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Analyzes a string object
    fn analyze_string(
        &self,
        string: &str,
        findings: &mut Vec<Finding>,
    ) -> Result<(), PdfError> {
        // Check for potentially malicious strings
        if string.contains("javascript:") || string.contains("eval(") {
            findings.push(Finding {
                id: uuid::Uuid::new_v4().to_string(),
                description: "Potentially malicious JavaScript in string".into(),
                risk_level: RiskLevel::High,
                path: "".into(),
                value: string.to_string(),
            });
        }

        // Check for URLs
        if string.starts_with("http://") || string.starts_with("https://") {
            findings.push(Finding {
                id: uuid::Uuid::new_v4().to_string(),
                description: "URL found in string".into(),
                risk_level: RiskLevel::Medium,
                path: "".into(),
                value: string.to_string(),
            });
        }

        Ok(())
    }

    /// Analyzes a generic PDF object value
    fn analyze_value(
        &self,
        value: &PdfObject,
        path: &str,
        findings: &mut Vec<Finding>,
    ) -> Result<(), PdfError> {
        match value {
            PdfObject::String(s) => self.analyze_string(s, findings)?,
            PdfObject::Name(name) if self.is_risky_name(name) => {
                findings.push(Finding {
                    id: uuid::Uuid::new_v4().to_string(),
                    description: "Potentially risky name found".into(),
                    risk_level: RiskLevel::Medium,
                    path: path.to_string(),
                    value: name.clone(),
                });
            }
            _ => {}
        }

        Ok(())
    }

    /// Checks if a name is potentially risky
    fn is_risky_name(&self, name: &str) -> bool {
        name.contains("JavaScript") || 
        name.contains("Launch") || 
        name.contains("URI") ||
        name.contains("SubmitForm")
    }

    /// Creates forensic artifacts from analysis findings
    fn create_artifacts(&self, analysis: &ObjectAnalysis) -> Vec<ForensicArtifact> {
        let mut artifacts = Vec::new();

        for finding in &analysis.findings {
            let mut metadata = HashMap::new();
            metadata.insert("object_type".into(), format!("{:?}", analysis.object_type));
            metadata.insert("object_id".into(), analysis.id.clone());
            metadata.insert("depth".into(), analysis.depth.to_string());
            metadata.insert("path".into(), finding.path.clone());
            metadata.insert("value".into(), finding.value.clone());

            artifacts.push(ForensicArtifact {
                id: finding.id.clone(),
                artifact_type: match analysis.object_type {
                    ObjectType::Stream => ArtifactType::Content,
                    _ => ArtifactType::Structure,
                },
                location: format!("{}:{}", analysis.id, finding.path),
                description: finding.description.clone(),
                risk_level: finding.risk_level,
                remediation: self.generate_remediation(finding, &analysis.object_type),
                metadata,
                detection_timestamp: chrono::Utc::now(),
                hash: self.calculate_hash(&finding.value),
            });
        }

        artifacts
    }

    /// Generates remediation advice
    fn generate_remediation(&self, finding: &Finding, object_type: &ObjectType) -> String {
        match (finding.risk_level, object_type) {
            (RiskLevel::Critical, _) => format!(
                "Remove or disable potentially malicious content at {}. \
                Review all associated actions and scripts.",
                finding.path
            ),
            (RiskLevel::High, ObjectType::Stream) => format!(
                "Decode and inspect stream content at {}. \
                Remove or sanitize if suspicious.",
                finding.path
            ),
            (RiskLevel::High, _) => format!(
                "Review and potentially remove suspicious object at {}.",
                finding.path
            ),
            (_, _) => format!(
                "Inspect and validate content at {} for potential risks.",
                finding.path
            ),
        }
    }

    /// Calculates hash of a value
    fn calculate_hash(&self, value: &str) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    #[test]
    async fn test_risky_key_detection() {
        let scanner = ObjectScanner::new(ScannerConfig::default());
        let mut dict = Dictionary::new();
        dict.insert("JavaScript", PdfObject::String("alert()".into()));
        
        let mut findings = Vec::new();
        let mut context = ScanContext::new();
        scanner.analyze_dictionary(&dict, &mut findings, &mut context).unwrap();
        
        assert!(!findings.is_empty());
        assert_eq!(findings[0].risk_level, RiskLevel::Critical);
    }

    #[test]
    async fn test_stream_analysis() {
        let scanner = ObjectScanner::new(ScannerConfig::default());
        let mut dict = Dictionary::new();
        dict.insert("Filter", PdfObject::Name("Crypt".into()));
        
        let mut findings = Vec::new();
        scanner.analyze_stream_dict(&dict, &mut findings).unwrap();
        
        assert!(!findings.is_empty());
        assert_eq!(findings[0].risk_level, RiskLevel::High);
    }

    #[test]
    async fn test_string_analysis() {
        let scanner = ObjectScanner::new(ScannerConfig::default());
        let mut findings = Vec::new();
        
        scanner.analyze_string("javascript:alert()", &mut findings).unwrap();
        assert!(!findings.is_empty());
        
        findings.clear();
        scanner.analyze_string("https://example.com", &mut findings).unwrap();
        assert!(!findings.is_empty());
    }

    #[test]
    async fn test_recursion_limit() {
        let scanner = ObjectScanner::new(ScannerConfig::default());
        let mut context = ScanContext::new();
        context.depth = scanner.config.max_recursion_depth;
        
        let obj = PdfObject::Dictionary(Dictionary::new());
        let result = scanner.scan_object(&obj, &mut context).await;
        
        assert!(result.is_err());
    }
}
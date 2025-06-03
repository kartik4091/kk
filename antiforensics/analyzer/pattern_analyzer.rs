//! Pattern analyser for PDF document analysis
//! Author: kartik4091
//! Created: 2025-06-03 04:45:32 UTC
//! This module provides pattern analysis capabilities for PDF documents,
//! including regex pattern matching, byte sequence analysis, and entropy analysis.

use std::{
    sync::Arc,
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};
use async_trait::async_trait;
use regex::bytes::{RegexSet, RegexSetBuilder};
use tracing::{info, warn, error, debug, trace, instrument};

use super::AnalyserConfig;
use crate::antiforensics::{
    Document,
    PdfError,
    RiskLevel,
    ForensicArtifact,
    ArtifactType,
};

/// Pattern analyser implementation
pub struct PatternAnalyser {
    /// Analyser configuration
    config: Arc<AnalyserConfig>,
    /// Compiled regex patterns
    patterns: RegexSet,
    /// Pattern metadata
    pattern_metadata: HashMap<usize, PatternMetadata>,
    /// Entropy thresholds
    entropy_thresholds: EntropyThresholds,
}

/// Pattern metadata
#[derive(Debug, Clone)]
struct PatternMetadata {
    /// Pattern identifier
    id: String,
    /// Pattern description
    description: String,
    /// Risk level
    risk_level: RiskLevel,
    /// Pattern category
    category: String,
    /// Context size
    context_size: usize,
}

/// Entropy thresholds
#[derive(Debug, Clone)]
struct EntropyThresholds {
    /// Low entropy threshold
    low: f64,
    /// Medium entropy threshold
    medium: f64,
    /// High entropy threshold
    high: f64,
}

impl PatternAnalyser {
    /// Creates a new pattern analyser instance
    #[instrument(skip(config))]
    pub fn new(config: AnalyserConfig) -> Self {
        debug!("Initializing PatternAnalyser");

        let (patterns, metadata) = Self::compile_patterns();

        Self {
            config: Arc::new(config),
            patterns,
            pattern_metadata: metadata,
            entropy_thresholds: EntropyThresholds {
                low: 3.0,
                medium: 5.0,
                high: 7.0,
            },
        }
    }

    /// Analyzes patterns in a document
    #[instrument(skip(self, doc, existing_artifacts), err(Display))]
    pub async fn analyze(
        &self,
        doc: &Document,
        existing_artifacts: &[ForensicArtifact],
    ) -> Result<Vec<ForensicArtifact>, PdfError> {
        let mut artifacts = Vec::new();

        // Analyze document content
        let content = doc.get_raw_content()?;
        
        // Find pattern matches
        let matches = self.find_patterns(&content)?;
        
        // Convert matches to artifacts
        for (pattern_idx, locations) in matches {
            if let Some(metadata) = self.pattern_metadata.get(&pattern_idx) {
                for location in locations {
                    // Extract context around match
                    let context = self.extract_context(&content, location, metadata.context_size);
                    
                    // Calculate entropy for the matched region
                    let entropy = self.calculate_entropy(&context);
                    
                    // Create artifact metadata
                    let mut artifact_metadata = HashMap::new();
                    artifact_metadata.insert("offset".into(), location.to_string());
                    artifact_metadata.insert("length".into(), context.len().to_string());
                    artifact_metadata.insert("entropy".into(), entropy.to_string());
                    artifact_metadata.insert("category".into(), metadata.category.clone());
                    
                    // Create artifact
                    artifacts.push(ForensicArtifact {
                        id: uuid::Uuid::new_v4().to_string(),
                        artifact_type: ArtifactType::Pattern,
                        location: format!("pattern_match:{}", location),
                        description: format!("{} (Entropy: {:.2})", metadata.description, entropy),
                        risk_level: self.adjust_risk_level(metadata.risk_level, entropy),
                        remediation: self.generate_remediation(metadata, entropy),
                        metadata: artifact_metadata,
                        detection_timestamp: chrono::Utc::now(),
                        hash: self.calculate_hash(&context),
                    });
                }
            }
        }

        // Correlate with existing artifacts
        self.correlate_artifacts(&mut artifacts, existing_artifacts);

        Ok(artifacts)
    }

    /// Compiles regex patterns and their metadata
    fn compile_patterns() -> (RegexSet, HashMap<usize, PatternMetadata>) {
        let mut patterns = Vec::new();
        let mut metadata = HashMap::new();

        // Shellcode patterns
        patterns.push(r"(?-u)\x90{20,}"); // NOP sled
        metadata.insert(0, PatternMetadata {
            id: "PATTERN-001".into(),
            description: "NOP sled detected".into(),
            risk_level: RiskLevel::Critical,
            category: "shellcode".into(),
            context_size: 100,
        });

        // Encoded content patterns
        patterns.push(r"base64:[a-zA-Z0-9+/]{20,}={0,2}");
        metadata.insert(1, PatternMetadata {
            id: "PATTERN-002".into(),
            description: "Base64 encoded content detected".into(),
            risk_level: RiskLevel::Medium,
            category: "encoding".into(),
            context_size: 200,
        });

        // Obfuscated JavaScript patterns
        patterns.push(r"eval\s*\(|String\.fromCharCode|unescape\s*\(");
        metadata.insert(2, PatternMetadata {
            id: "PATTERN-003".into(),
            description: "Potentially obfuscated JavaScript detected".into(),
            risk_level: RiskLevel::High,
            category: "obfuscation".into(),
            context_size: 150,
        });

        // URL patterns
        patterns.push(r"https?://[^\s/$.?#].[^\s]*");
        metadata.insert(3, PatternMetadata {
            id: "PATTERN-004".into(),
            description: "URL detected".into(),
            risk_level: RiskLevel::Low,
            category: "network".into(),
            context_size: 100,
        });

        // Command injection patterns
        patterns.push(r"system\s*\(|exec\s*\(|spawn\s*\(");
        metadata.insert(4, PatternMetadata {
            id: "PATTERN-005".into(),
            description: "Potential command injection detected".into(),
            risk_level: RiskLevel::Critical,
            category: "injection".into(),
            context_size: 150,
        });

        let regex_set = RegexSetBuilder::new(patterns)
            .case_insensitive(true)
            .dot_matches_new_line(true)
            .build()
            .expect("Failed to compile regex patterns");

        (regex_set, metadata)
    }

    /// Finds pattern matches in content
    fn find_patterns(&self, content: &[u8]) -> Result<HashMap<usize, Vec<usize>>, PdfError> {
        let mut matches = HashMap::new();
        
        // Find all matches for each pattern
        for pattern_idx in self.patterns.matches(content).iter() {
            let pattern = self.patterns.patterns()[pattern_idx];
            
            // Find all occurrences of the pattern
            if let Ok(regex) = regex::bytes::Regex::new(pattern) {
                let locations: Vec<usize> = regex.find_iter(content)
                    .map(|m| m.start())
                    .collect();
                
                if !locations.is_empty() {
                    matches.insert(pattern_idx, locations);
                }
            }
        }

        Ok(matches)
    }

    /// Extracts context around a match
    fn extract_context(&self, content: &[u8], location: usize, context_size: usize) -> Vec<u8> {
        let start = location.saturating_sub(context_size);
        let end = (location + context_size).min(content.len());
        content[start..end].to_vec()
    }

    /// Calculates entropy of a byte sequence
    fn calculate_entropy(&self, data: &[u8]) -> f64 {
        let mut frequencies = [0.0f64; 256];
        let len = data.len() as f64;

        // Calculate byte frequencies
        for &byte in data {
            frequencies[byte as usize] += 1.0;
        }

        // Calculate entropy
        -frequencies.iter()
            .filter(|&&freq| freq > 0.0)
            .map(|&freq| {
                let p = freq / len;
                p * p.log2()
            })
            .sum::<f64>()
    }

    /// Adjusts risk level based on entropy
    fn adjust_risk_level(&self, base_level: RiskLevel, entropy: f64) -> RiskLevel {
        match (base_level, entropy) {
            (RiskLevel::Low, e) if e > self.entropy_thresholds.high => RiskLevel::Medium,
            (RiskLevel::Medium, e) if e > self.entropy_thresholds.high => RiskLevel::High,
            (RiskLevel::High, e) if e > self.entropy_thresholds.high => RiskLevel::Critical,
            (level, _) => level,
        }
    }

    /// Generates remediation advice
    fn generate_remediation(&self, metadata: &PatternMetadata, entropy: f64) -> String {
        match (metadata.category.as_str(), entropy) {
            ("shellcode", _) => "Remove shellcode and review code execution vectors".into(),
            ("encoding", e) if e > self.entropy_thresholds.high => 
                "Decode and review high-entropy encoded content".into(),
            ("encoding", _) => "Review encoded content for sensitive information".into(),
            ("obfuscation", _) => "Deobfuscate and review JavaScript code".into(),
            ("network", _) => "Validate and review network endpoints".into(),
            ("injection", _) => "Remove command injection vectors and sanitize inputs".into(),
            (_, _) => "Review and validate suspicious pattern".into(),
        }
    }

    /// Correlates pattern artifacts with existing artifacts
    fn correlate_artifacts(
        &self,
        pattern_artifacts: &mut Vec<ForensicArtifact>,
        existing_artifacts: &[ForensicArtifact],
    ) {
        for pattern_artifact in pattern_artifacts.iter_mut() {
            // Find related artifacts within the same region
            let pattern_offset = pattern_artifact.metadata.get("offset")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);

            for existing in existing_artifacts {
                if let Some(existing_offset) = existing.metadata.get("offset")
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    // Check if artifacts are within proximity
                    if (pattern_offset as i64 - existing_offset as i64).abs() < 100 {
                        pattern_artifact.metadata.insert(
                            format!("related_artifact_{}", existing.id),
                            existing.description.clone(),
                        );

                        // Escalate risk level if related artifact has higher risk
                        if existing.risk_level > pattern_artifact.risk_level {
                            pattern_artifact.risk_level = existing.risk_level;
                        }
                    }
                }
            }
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
    async fn test_pattern_detection() {
        let analyser = PatternAnalyser::new(AnalyserConfig::default());
        let mut doc = Document::new();
        doc.add_content(b"eval('alert(1)')");
        
        let artifacts = analyser.analyze(&doc, &[]).await.unwrap();
        assert!(!artifacts.is_empty());
        assert_eq!(artifacts[0].risk_level, RiskLevel::High);
    }

    #[test]
    async fn test_entropy_calculation() {
        let analyser = PatternAnalyser::new(AnalyserConfig::default());
        
        let low_entropy = b"AAAAAAAAAA";
        let high_entropy = b"1X#m9k*P3q";
        
        let low_score = analyser.calculate_entropy(low_entropy);
        let high_score = analyser.calculate_entropy(high_entropy);
        
        assert!(low_score < high_score);
    }

    #[test]
    async fn test_pattern_correlation() {
        let analyser = PatternAnalyser::new(AnalyserConfig::default());
        let mut doc = Document::new();
        doc.add_content(b"eval('alert(1)')");
        
        let existing_artifacts = vec![
            ForensicArtifact {
                risk_level: RiskLevel::Critical,
                metadata: {
                    let mut m = HashMap::new();
                    m.insert("offset".into(), "0".into());
                    m
                },
                ..Default::default()
            },
        ];

        let mut artifacts = analyser.analyze(&doc, &existing_artifacts).await.unwrap();
        assert_eq!(artifacts[0].risk_level, RiskLevel::Critical);
    }

    #[test]
    async fn test_context_extraction() {
        let analyser = PatternAnalyser::new(AnalyserConfig::default());
        let content = b"prefix_eval('alert(1)')_suffix";
        
        let context = analyser.extract_context(content, 7, 5);
        assert_eq!(context.len(), 11);
    }
                                             }

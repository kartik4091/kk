//! Metadata Scanner Implementation
//! Author: kartik4091
//! Created: 2025-06-03 08:50:07 UTC

use super::*;
use crate::utils::{metrics::Metrics, cache::Cache};
use std::{
    sync::Arc,
    path::PathBuf,
    time::{Duration, Instant},
    collections::{HashMap, HashSet, BTreeMap},
};
use tokio::{
    sync::{RwLock, Semaphore, broadcast},
    fs::{self, File},
    io::{BufReader, AsyncReadExt},
};
use tracing::{info, warn, error, debug, instrument};
use regex::Regex;
use lazy_static::lazy_static;

/// Metadata scanner configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataScannerConfig {
    /// Base scanner configuration
    pub base: ScannerConfig,
    /// Sensitive patterns to scan for
    pub sensitive_patterns: HashMap<String, String>,
    /// Required metadata fields
    pub required_fields: HashSet<String>,
    /// Metadata validation rules
    pub validation_rules: HashMap<String, String>,
    /// Privacy risk threshold (0.0 - 1.0)
    pub privacy_threshold: f64,
}

/// Metadata scanner state
#[derive(Debug)]
struct MetadataScannerState {
    /// Scan cache
    cache: HashMap<PathBuf, CachedMetadataScan>,
    /// Compiled patterns
    patterns: HashMap<String, Regex>,
    /// Statistics
    stats: MetadataStats,
}

/// Metadata scan statistics
#[derive(Debug, Default)]
struct MetadataStats {
    files_scanned: u64,
    fields_analyzed: u64,
    sensitive_findings: u64,
    validation_failures: u64,
    avg_scan_time: Duration,
}

/// Cached metadata scan
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedMetadataScan {
    results: ScanResult,
    timestamp: chrono::DateTime<chrono::Utc>,
    hash: String,
}

/// Privacy risk levels
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
enum PrivacyRisk {
    None = 0,
    Low = 1,
    Medium = 2,
    High = 3,
    Critical = 4,
}

lazy_static! {
    static ref COMMON_PATTERNS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("email", r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}");
        m.insert("phone", r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b");
        m.insert("ssn", r"\b\d{3}-\d{2}-\d{4}\b");
        m.insert("credit_card", r"\b\d{4}[-\s]?\d{4}[-\s]?\d{4}[-\s]?\d{4}\b");
        m.insert("ip_address", r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b");
        m.insert("date", r"\d{4}-\d{2}-\d{2}");
        m
    };
}

pub struct MetadataScanner {
    base: Arc<BaseScanner>,
    config: Arc<MetadataScannerConfig>,
    state: Arc<RwLock<MetadataScannerState>>,
    metrics: Arc<Metrics>,
    cache: Arc<Cache<CachedMetadataScan>>,
}

impl MetadataScanner {
    /// Creates a new metadata scanner
    pub fn new(config: MetadataScannerConfig) -> Self {
        let patterns = Self::compile_patterns(&config.sensitive_patterns);
        
        Self {
            base: Arc::new(BaseScanner::new(config.base.clone())),
            config: Arc::new(config),
            state: Arc::new(RwLock::new(MetadataScannerState {
                cache: HashMap::new(),
                patterns,
                stats: MetadataStats::default(),
            })),
            metrics: Arc::new(Metrics::new()),
            cache: Arc::new(Cache::new(Duration::from_secs(3600))), // 1 hour cache
        }
    }

    /// Compiles regex patterns
    fn compile_patterns(patterns: &HashMap<String, String>) -> HashMap<String, Regex> {
        patterns.iter()
            .filter_map(|(name, pattern)| {
                Regex::new(pattern)
                    .map(|r| (name.clone(), r))
                    .ok()
            })
            .collect()
    }

    /// Extracts metadata from file
    #[instrument(skip(self, data))]
    async fn extract_metadata(&self, path: &PathBuf, data: &[u8]) -> Result<HashMap<String, String>> {
        let start = Instant::now();
        let mut metadata = HashMap::new();

        // Extract file system metadata
        if let Ok(fs_metadata) = fs::metadata(path).await {
            metadata.insert("size".into(), fs_metadata.len().to_string());
            metadata.insert("modified".into(), 
                fs_metadata.modified().ok()
                    .and_then(|t| t.into_std().ok())
                    .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
                    .unwrap_or_default()
            );
            metadata.insert("created".into(),
                fs_metadata.created().ok()
                    .and_then(|t| t.into_std().ok())
                    .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
                    .unwrap_or_default()
            );
        }

        // Extract file type metadata
        if let Some(ext) = path.extension() {
            metadata.insert("extension".into(), ext.to_string_lossy().to_string());
        }

        // Extract MIME type
        if let Some(mime) = mime_guess::from_path(path).first() {
            metadata.insert("mime_type".into(), mime.to_string());
        }

        self.metrics.record_operation("metadata_extraction", start.elapsed()).await;
        Ok(metadata)
    }

    /// Analyzes metadata for sensitive information
    #[instrument(skip(self, metadata))]
    async fn analyze_sensitive_info(&self, metadata: &HashMap<String, String>) -> Vec<ScanFinding> {
        let mut findings = Vec::new();
        let state = self.state.read().await;

        for (field, value) in metadata {
            // Check custom patterns
            for (pattern_name, pattern) in &state.patterns {
                if pattern.is_match(value) {
                    findings.push(ScanFinding {
                        severity: Severity::High,
                        category: Category::Security,
                        description: format!("Sensitive information found: {}", pattern_name),
                        location: format!("Metadata field: {}", field),
                        recommendation: "Remove or redact sensitive information".into(),
                        timestamp: chrono::Utc::now(),
                    });
                }
            }

            // Check common patterns
            for (pattern_name, pattern_str) in COMMON_PATTERNS.iter() {
                if Regex::new(pattern_str).unwrap().is_match(value) {
                    findings.push(ScanFinding {
                        severity: Severity::High,
                        category: Category::Security,
                        description: format!("Common sensitive pattern found: {}", pattern_name),
                        location: format!("Metadata field: {}", field),
                        recommendation: "Review and remove sensitive information".into(),
                        timestamp: chrono::Utc::now(),
                    });
                }
            }
        }

        findings
    }

    /// Validates metadata against rules
    #[instrument(skip(self, metadata))]
    async fn validate_metadata(&self, metadata: &HashMap<String, String>) -> Vec<ScanFinding> {
        let mut findings = Vec::new();

        // Check required fields
        for field in &self.config.required_fields {
            if !metadata.contains_key(field) {
                findings.push(ScanFinding {
                    severity: Severity::Medium,
                    category: Category::Content,
                    description: format!("Required metadata field missing: {}", field),
                    location: "Metadata structure".into(),
                    recommendation: format!("Add required field: {}", field),
                    timestamp: chrono::Utc::now(),
                });
            }
        }

        // Validate against rules
        for (field, rule) in &self.config.validation_rules {
            if let Some(value) = metadata.get(field) {
                if let Ok(pattern) = Regex::new(rule) {
                    if !pattern.is_match(value) {
                        findings.push(ScanFinding {
                            severity: Severity::Low,
                            category: Category::Content,
                            description: format!("Metadata field validation failed: {}", field),
                            location: format!("Field: {}", field),
                            recommendation: format!("Update field to match required format: {}", rule),
                            timestamp: chrono::Utc::now(),
                        });
                    }
                }
            }
        }

        findings
    }

    /// Calculates privacy risk score
    fn calculate_privacy_risk(&self, findings: &[ScanFinding]) -> PrivacyRisk {
        let risk_score = findings.iter()
            .map(|f| match f.severity {
                Severity::Critical => 1.0,
                Severity::High => 0.8,
                Severity::Medium => 0.5,
                Severity::Low => 0.2,
                Severity::Info => 0.1,
            })
            .sum::<f64>() / findings.len() as f64;

        match risk_score {
            s if s >= 0.8 => PrivacyRisk::Critical,
            s if s >= 0.6 => PrivacyRisk::High,
            s if s >= 0.4 => PrivacyRisk::Medium,
            s if s >= 0.2 => PrivacyRisk::Low,
            _ => PrivacyRisk::None,
        }
    }
}

#[async_trait]
impl Scanner for MetadataScanner {
    #[instrument(skip(self))]
    async fn scan_file(&self, path: &PathBuf) -> Result<ScanResult> {
        let start = Instant::now();

        // Get rate limiting permit
        let _permit = self.base.semaphore.acquire().await
            .map_err(|e| ScannerError::Internal(e.to_string()))?;

        // Validate input
        self.validate(path).await?;

        // Read file
        let data = fs::read(path).await?;
        let hash = format!("{:x}", md5::compute(&data));

        // Check cache
        let cache_key = format!("metadata_scan_{}", hash);
        if let Some(cached) = self.cache.get(&cache_key).await {
            return Ok(cached.results);
        }

        // Extract metadata
        let metadata = self.extract_metadata(path, &data).await?;

        // Analyze metadata
        let mut findings = Vec::new();
        findings.extend(self.analyze_sensitive_info(&metadata).await);
        findings.extend(self.validate_metadata(&metadata).await);

        // Calculate privacy risk
        let privacy_risk = self.calculate_privacy_risk(&findings);
        if privacy_risk as u8 >= PrivacyRisk::High as u8 {
            findings.push(ScanFinding {
                severity: Severity::High,
                category: Category::Security,
                description: format!("High privacy risk detected: {:?}", privacy_risk),
                location: "Overall metadata".into(),
                recommendation: "Review and remove sensitive information".into(),
                timestamp: chrono::Utc::now(),
            });
        }

        // Update statistics
        let duration = start.elapsed();
        self.base.update_metrics(duration, true).await;

        let mut state = self.state.write().await;
        state.stats.files_scanned += 1;
        state.stats.fields_analyzed += metadata.len() as u64;
        state.stats.sensitive_findings += findings.len() as u64;
        state.stats.avg_scan_time = (state.stats.avg_scan_time + duration) / 2;

        // Prepare result
        let result = ScanResult {
            path: path.clone(),
            size: data.len() as u64,
            file_type: "Metadata".into(),
            findings,
            metadata,
            metrics: ScanMetrics {
                duration,
                memory_usage: std::mem::size_of_val(&data),
                cpu_usage: 0.0,
            },
        };

        // Cache result
        let cache_entry = CachedMetadataScan {
            results: result.clone(),
            timestamp: chrono::Utc::now(),
            hash,
        };
        self.cache.set(cache_key, cache_entry).await;

        Ok(result)
    }

    #[instrument(skip(self))]
    async fn validate(&self, path: &PathBuf) -> Result<()> {
        self.base.validate_file(path).await
    }

    #[instrument(skip(self))]
    async fn cleanup(&self) -> Result<()> {
        self.cache.clear().await;
        let mut state = self.state.write().await;
        state.cache.clear();
        state.stats = MetadataStats::default();
        Ok(())
    }

    #[instrument(skip(self))]
    async fn get_stats(&self) -> Result<ScannerMetrics> {
        let state = self.state.read().await;
        Ok(ScannerMetrics {
            total_scans: state.stats.files_scanned,
            successful_scans: state.stats.files_scanned,
            failed_scans: 0,
            total_scan_time: state.stats.avg_scan_time * state.stats.files_scanned,
            avg_scan_time: state.stats.avg_scan_time,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn create_test_config() -> MetadataScannerConfig {
        MetadataScannerConfig {
            base: ScannerConfig::default(),
            sensitive_patterns: [
                ("test_pattern".into(), r"secret\d+".into()),
            ].iter().cloned().collect(),
            required_fields: ["author", "created"].iter().map(|&s| s.into()).collect(),
            validation_rules: [
                ("author".into(), r"^[a-zA-Z\s]{2,50}$".into()),
            ].iter().cloned().collect(),
            privacy_threshold: 0.7,
        }
    }

    #[tokio::test]
    async fn test_metadata_extraction() {
        let scanner = MetadataScanner::new(create_test_config());
        let file = NamedTempFile::new().unwrap();
        let path = PathBuf::from(file.path());
        
        let metadata = scanner.extract_metadata(&path, &[]).await.unwrap();
        assert!(metadata.contains_key("size"));
        assert!(metadata.contains_key("modified"));
    }

    #[tokio::test]
    async fn test_sensitive_info_detection() {
        let scanner = MetadataScanner::new(create_test_config());
        let metadata = [
            ("field".into(), "secret123".into()),
        ].iter().cloned().collect();
        
        let findings = scanner.analyze_sensitive_info(&metadata).await;
        assert!(!findings.is_empty());
        assert_eq!(findings[0].severity, Severity::High);
    }

    #[tokio::test]
    async fn test_metadata_validation() {
        let scanner = MetadataScanner::new(create_test_config());
        let metadata = HashMap::new();
        
        let findings = scanner.validate_metadata(&metadata).await;
        assert!(!findings.is_empty());
    }

    #[tokio::test]
    async fn test_privacy_risk_calculation() {
        let scanner = MetadataScanner::new(create_test_config());
        let findings = vec![
            ScanFinding {
                severity: Severity::High,
                category: Category::Security,
                description: "Test finding".into(),
                location: "Test".into(),
                recommendation: "Test".into(),
                timestamp: chrono::Utc::now(),
            },
        ];
        
        let risk = scanner.calculate_privacy_risk(&findings);
        assert!(risk >= PrivacyRisk::Medium);
    }

    #[tokio::test]
    async fn test_concurrent_scans() {
        let scanner = MetadataScanner::new(MetadataScannerConfig {
            base: ScannerConfig {
                max_concurrent_scans: 2,
                ..ScannerConfig::default()
            },
            ..create_test_config()
        });

        let file = NamedTempFile::new().unwrap();
        let path = PathBuf::from(file.path());

        let handles: Vec<_> = (0..4).map(|_| {
            let scanner = scanner.clone();
            let path = path.clone();
            tokio::spawn(async move {
                scanner.scan_file(&path).await
            })
        }).collect();

        let results = futures::future::join_all(handles).await;
        for result in results {
            assert!(result.unwrap().is_ok());
        }
    }
                      }

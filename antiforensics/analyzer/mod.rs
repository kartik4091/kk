//! Analyzer module for PDF document risk assessment
//! Author: kartik4091
//! Created: 2025-06-03 04:16:03 UTC
//! This module provides risk analysis capabilities for PDF documents,
//! identifying potential security risks and forensic artifacts.

use std::{
    sync::Arc,
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::sync::{RwLock, Semaphore};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use tracing::{info, warn, error, debug, trace, instrument};

mod risk_analyzer;
mod pattern_analyzer;

pub use risk_analyzer::RiskAnalyzer;
pub use pattern_analyzer::PatternAnalyzer;
use crate::antiforensics::{Document, PdfError, RiskLevel, ForensicArtifact};

/// Configuration for the analyzer component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzerConfig {
    /// Maximum number of concurrent analysis operations
    pub max_concurrent_analyses: usize,
    /// Size of the analysis cache in megabytes
    pub cache_size_mb: usize,
    /// Timeout for analysis operations
    pub analysis_timeout: Duration,
    /// Whether to use strict analysis mode
    pub strict_mode: bool,
    /// Custom analysis rules in YAML format
    pub custom_rules: Option<String>,
    /// Minimum confidence level for detection (0.0 - 1.0)
    pub min_confidence: f64,
    /// Maximum memory usage per analysis in megabytes
    pub max_memory_per_analysis: usize,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_analyses: 4,
            cache_size_mb: 512,
            analysis_timeout: Duration::from_secs(120),
            strict_mode: true,
            custom_rules: None,
            min_confidence: 0.75,
            max_memory_per_analysis: 256,
        }
    }
}

/// Result of document analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    /// Unique identifier for the analysis
    pub id: String,
    /// Timestamp of the analysis
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Overall risk level
    pub risk_level: RiskLevel,
    /// Detected forensic artifacts
    pub artifacts: Vec<ForensicArtifact>,
    /// Recommendations for mitigation
    pub recommendations: Vec<String>,
    /// Analysis duration
    pub duration: Duration,
    /// Analysis metadata
    pub metadata: HashMap<String, String>,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
}

/// Interface for PDF document analyzers
#[async_trait]
pub trait Analyzer: Send + Sync {
    /// Analyzes a PDF document for forensic artifacts and risks
    async fn analyze(&self, doc: &Document, scan_result: &ScanResult) -> Result<AnalysisResult, PdfError>;
    
    /// Gets the analyzer's current metrics
    async fn get_metrics(&self) -> AnalyzerMetrics;
    
    /// Validates analysis results
    fn validate_result(&self, result: &AnalysisResult) -> bool;
}

/// Metrics for analyzer performance monitoring
#[derive(Debug, Clone, Default, Serialize)]
pub struct AnalyzerMetrics {
    /// Total number of analyses performed
    pub total_analyses: usize,
    /// Number of successful analyses
    pub successful_analyses: usize,
    /// Number of failed analyses
    pub failed_analyses: usize,
    /// Total analysis time
    pub total_analysis_time: Duration,
    /// Average analysis time
    pub average_analysis_time: Duration,
    /// Cache hit rate
    pub cache_hit_rate: f64,
    /// Memory usage in bytes
    pub memory_usage: usize,
    /// Error rate
    pub error_rate: f64,
}

/// Cache for analysis results
#[derive(Debug)]
struct AnalysisCache {
    /// Cached analysis results
    entries: HashMap<String, CacheEntry>,
    /// Maximum cache size in bytes
    max_size: usize,
    /// Current cache size in bytes
    current_size: usize,
}

/// Entry in the analysis cache
#[derive(Debug, Clone)]
struct CacheEntry {
    /// Cached analysis result
    result: AnalysisResult,
    /// When the entry was last accessed
    last_accessed: chrono::DateTime<chrono::Utc>,
    /// When the entry expires
    expires_at: chrono::DateTime<chrono::Utc>,
    /// Size of the entry in bytes
    size: usize,
}

impl AnalysisCache {
    /// Creates a new analysis cache
    fn new(max_size_mb: usize) -> Self {
        Self {
            entries: HashMap::new(),
            max_size: max_size_mb * 1024 * 1024,
            current_size: 0,
        }
    }

    /// Gets an entry from the cache
    fn get(&mut self, key: &str) -> Option<AnalysisResult> {
        if let Some(entry) = self.entries.get_mut(key) {
            if entry.expires_at > chrono::Utc::now() {
                entry.last_accessed = chrono::Utc::now();
                return Some(entry.result.clone());
            }
            // Remove expired entry
            self.remove(key);
        }
        None
    }

    /// Puts an entry into the cache
    fn put(&mut self, key: String, result: AnalysisResult, ttl: Duration) {
        let size = bincode::serialize(&result).map(|v| v.len()).unwrap_or(0);
        
        // Remove entries if cache is full
        while self.current_size + size > self.max_size {
            if let Some(oldest) = self.get_oldest_entry() {
                self.remove(&oldest);
            } else {
                break;
            }
        }

        let entry = CacheEntry {
            result,
            last_accessed: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::from_std(ttl).unwrap(),
            size,
        };

        self.current_size += size;
        self.entries.insert(key, entry);
    }

    /// Removes an entry from the cache
    fn remove(&mut self, key: &str) {
        if let Some(entry) = self.entries.remove(key) {
            self.current_size -= entry.size;
        }
    }

    /// Gets the key of the oldest entry
    fn get_oldest_entry(&self) -> Option<String> {
        self.entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(key, _)| key.clone())
    }
}

/// Base implementation for analyzers
pub struct BaseAnalyzer {
    /// Analyzer configuration
    config: Arc<AnalyzerConfig>,
    /// Analysis cache
    cache: Arc<RwLock<AnalysisCache>>,
    /// Metrics
    metrics: Arc<RwLock<AnalyzerMetrics>>,
    /// Semaphore for limiting concurrent analyses
    analysis_semaphore: Arc<Semaphore>,
}

impl BaseAnalyzer {
    /// Creates a new base analyzer
    pub fn new(config: AnalyzerConfig) -> Self {
        Self {
            config: Arc::new(config.clone()),
            cache: Arc::new(RwLock::new(AnalysisCache::new(config.cache_size_mb))),
            metrics: Arc::new(RwLock::new(AnalyzerMetrics::default())),
            analysis_semaphore: Arc::new(Semaphore::new(config.max_concurrent_analyses)),
        }
    }

    /// Updates analyzer metrics
    async fn update_metrics(&self, duration: Duration, success: bool) {
        let mut metrics = self.metrics.write().await;
        metrics.total_analyses += 1;
        if success {
            metrics.successful_analyses += 1;
        } else {
            metrics.failed_analyses += 1;
        }
        metrics.total_analysis_time += duration;
        metrics.average_analysis_time = metrics.total_analysis_time / metrics.total_analyses as u32;
        metrics.error_rate = metrics.failed_analyses as f64 / metrics.total_analyses as f64;
    }

    /// Generates a cache key for a document
    fn generate_cache_key(&self, doc: &Document) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(doc.get_id().unwrap_or_default());
        hasher.update(doc.calculate_hash());
        format!("{:x}", hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    #[test]
    async fn test_cache_operations() {
        let mut cache = AnalysisCache::new(1);
        
        let result = AnalysisResult {
            id: "test".into(),
            timestamp: chrono::Utc::now(),
            risk_level: RiskLevel::Low,
            artifacts: vec![],
            recommendations: vec![],
            duration: Duration::from_secs(1),
            metadata: HashMap::new(),
            confidence: 1.0,
        };

        cache.put("key1".into(), result.clone(), Duration::from_secs(60));
        assert!(cache.get("key1").is_some());
        assert!(cache.get("key2").is_none());
    }

    #[test]
    async fn test_cache_expiration() {
        let mut cache = AnalysisCache::new(1);
        
        let result = AnalysisResult {
            id: "test".into(),
            timestamp: chrono::Utc::now(),
            risk_level: RiskLevel::Low,
            artifacts: vec![],
            recommendations: vec![],
            duration: Duration::from_secs(1),
            metadata: HashMap::new(),
            confidence: 1.0,
        };

        cache.put("key1".into(), result, Duration::from_nanos(1));
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert!(cache.get("key1").is_none());
    }

    #[test]
    async fn test_cache_size_limit() {
        let mut cache = AnalysisCache::new(1); // 1MB limit
        
        let large_result = AnalysisResult {
            id: "test".into(),
            timestamp: chrono::Utc::now(),
            risk_level: RiskLevel::Low,
            artifacts: vec![],
            recommendations: vec!["x".repeat(1024 * 1024).to_string()], // 1MB recommendation
            duration: Duration::from_secs(1),
            metadata: HashMap::new(),
            confidence: 1.0,
        };

        cache.put("key1".into(), large_result.clone(), Duration::from_secs(60));
        cache.put("key2".into(), large_result.clone(), Duration::from_secs(60));
        
        // First entry should be evicted
        assert!(cache.get("key1").is_none());
        assert!(cache.get("key2").is_some());
    }

    #[test]
    async fn test_analyzer_config() {
        let config = AnalyzerConfig::default();
        assert!(config.min_confidence > 0.0 && config.min_confidence <= 1.0);
        assert!(config.max_concurrent_analyses > 0);
        assert!(config.cache_size_mb > 0);
    }
          }

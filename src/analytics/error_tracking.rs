// Auto-generated for kartik4091/kk
// Timestamp: 2025-06-02 05:12:46
// User: kartik4091

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::{debug, error, info, instrument, warn};
use crate::core::error::PdfError;

#[derive(Debug, thiserror::Error)]
pub enum ErrorTrackingError {
    #[error("Error collection failed: {0}")]
    CollectionError(String),
    
    #[error("Invalid error data: {0}")]
    InvalidError(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Analysis error: {0}")]
    AnalysisError(String),
    
    #[error(transparent)]
    Storage(#[from] std::io::Error),
    
    #[error(transparent)]
    Core(#[from] PdfError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorTrackingConfig {
    pub collection_interval: u32,
    pub retention_period: u32,
    pub severity_levels: Vec<String>,
    pub error_categories: Vec<String>,
    pub alert_thresholds: HashMap<String, u32>,
}

impl Default for ErrorTrackingConfig {
    fn default() -> Self {
        Self {
            collection_interval: 60,  // seconds
            retention_period: 30,     // days
            severity_levels: vec![
                "critical".to_string(),
                "error".to_string(),
                "warning".to_string(),
                "info".to_string(),
            ],
            error_categories: vec![
                "parsing".to_string(),
                "rendering".to_string(),
                "accessibility".to_string(),
                "network".to_string(),
                "security".to_string(),
            ],
            alert_thresholds: {
                let mut thresholds = HashMap::new();
                thresholds.insert("critical".to_string(), 1);
                thresholds.insert("error".to_string(), 10);
                thresholds.insert("warning".to_string(), 100);
                thresholds
            },
        }
    }
}

#[derive(Debug)]
pub struct ErrorTrackingManager {
    config: ErrorTrackingConfig,
    state: Arc<RwLock<ErrorTrackingState>>,
    metrics: Arc<ErrorTrackingMetrics>,
}

#[derive(Debug, Default)]
struct ErrorTrackingState {
    errors: Vec<ErrorRecord>,
    error_groups: HashMap<String, ErrorGroup>,
    active_alerts: Vec<ErrorAlert>,
    analysis_cache: HashMap<String, ErrorAnalysis>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRecord {
    id: String,
    timestamp: DateTime<Utc>,
    error_type: String,
    message: String,
    stack_trace: Option<String>,
    severity: String,
    category: String,
    context: ErrorContext,
    user_id: Option<String>,
    resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorContext {
    file_path: Option<String>,
    line_number: Option<u32>,
    component: Option<String>,
    environment: String,
    metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorGroup {
    id: String,
    error_type: String,
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    count: u32,
    examples: Vec<String>,
    status: ErrorGroupStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorGroupStatus {
    Active,
    Resolved,
    Ignored,
    UnderInvestigation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorAlert {
    id: String,
    error_group_id: String,
    severity: String,
    message: String,
    triggered_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    notification_sent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorAnalysis {
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
    total_errors: u32,
    error_rate: f64,
    severity_breakdown: HashMap<String, u32>,
    category_breakdown: HashMap<String, u32>,
    top_errors: Vec<ErrorSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorSummary {
    error_type: String,
    count: u32,
    last_occurrence: DateTime<Utc>,
    impact_score: f64,
}

#[derive(Debug)]
struct ErrorTrackingMetrics {
    total_errors: prometheus::IntCounter,
    errors_by_severity: prometheus::IntCounterVec,
    errors_by_category: prometheus::IntCounterVec,
    active_alerts: prometheus::Gauge,
    error_processing_time: prometheus::Histogram,
}

#[async_trait]
pub trait ErrorTracker {
    async fn track_error(&mut self, error: ErrorRecord) -> Result<(), ErrorTrackingError>;
    async fn get_error(&self, error_id: &str) -> Result<ErrorRecord, ErrorTrackingError>;
    async fn get_error_group(&self, group_id: &str) -> Result<ErrorGroup, ErrorTrackingError>;
    async fn analyze_errors(&self, period: Option<(DateTime<Utc>, DateTime<Utc>)>) -> Result<ErrorAnalysis, ErrorTrackingError>;
}

impl ErrorTrackingManager {
    pub fn new(config: ErrorTrackingConfig) -> Self {
        let metrics = Arc::new(ErrorTrackingMetrics::new());
        
        Self {
            config,
            state: Arc::new(RwLock::new(ErrorTrackingState::default())),
            metrics,
        }
    }

    #[instrument(skip(self))]
    pub async fn initialize(&self) -> Result<(), ErrorTrackingError> {
        info!("Initializing ErrorTrackingManager");
        Ok(())
    }

    async fn group_error(&self, error: &ErrorRecord) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&error.error_type, &mut hasher);
        if let Some(stack) = &error.stack_trace {
            std::hash::Hash::hash(&stack, &mut hasher);
        }
        format!("group-{}", std::hash::Hash::finish(&hasher))
    }

    async fn check_alert_thresholds(&self, group: &ErrorGroup) -> Option<ErrorAlert> {
        if let Some(&threshold) = self.config.alert_thresholds.get(&group.error_type) {
            if group.count >= threshold {
                return Some(ErrorAlert {
                    id: uuid::Uuid::new_v4().to_string(),
                    error_group_id: group.id.clone(),
                    severity: "high".to_string(),
                    message: format!("Error threshold exceeded for {}", group.error_type),
                    triggered_at: Utc::now(),
                    resolved_at: None,
                    notification_sent: false,
                });
            }
        }
        None
    }
}

#[async_trait]
impl ErrorTracker for ErrorTrackingManager {
    #[instrument(skip(self))]
    async fn track_error(&mut self, error: ErrorRecord) -> Result<(), ErrorTrackingError> {
        let timer = self.metrics.error_processing_time.start_timer();
        
        // Validate error data
        if !self.config.severity_levels.contains(&error.severity) {
            return Err(ErrorTrackingError::InvalidError(
                format!("Invalid severity level: {}", error.severity)
            ));
        }

        let group_id = self.group_error(&error).await;
        
        let mut state = self.state.write().await;
        
        // Update error group
        let group = state.error_groups.entry(group_id.clone()).or_insert_with(|| ErrorGroup {
            id: group_id.clone(),
            error_type: error.error_type.clone(),
            first_seen: error.timestamp,
            last_seen: error.timestamp,
            count: 0,
            examples: Vec::new(),
            status: ErrorGroupStatus::Active,
        });

        group.count += 1;
        group.last_seen = error.timestamp;
        if group.examples.len() < 5 {
            group.examples.push(error.id.clone());
        }

        // Check for alerts
        if let Some(alert) = self.check_alert_thresholds(group).await {
            state.active_alerts.push(alert);
        }

        // Store error record
        state.errors.push(error);

        // Update metrics
        self.metrics.total_errors.inc();
        self.metrics.errors_by_severity.with_label_values(&[&error.severity]).inc();
        self.metrics.errors_by_category.with_label_values(&[&error.category]).inc();
        
        timer.observe_duration();
        
        Ok(())
    }

    #[instrument(skip(self))]
    async fn get_error(&self, error_id: &str) -> Result<ErrorRecord, ErrorTrackingError> {
        let state = self.state.read().await;
        
        state.errors
            .iter()
            .find(|e| e.id == error_id)
            .cloned()
            .ok_or_else(|| ErrorTrackingError::InvalidError(
                format!("Error not found: {}", error_id)
            ))
    }

    #[instrument(skip(self))]
    async fn get_error_group(&self, group_id: &str) -> Result<ErrorGroup, ErrorTrackingError> {
        let state = self.state.read().await;
        
        state.error_groups
            .get(group_id)
            .cloned()
            .ok_or_else(|| ErrorTrackingError::InvalidError(
                format!("Error group not found: {}", group_id)
            ))
    }

    #[instrument(skip(self))]
    async fn analyze_errors(&self, period: Option<(DateTime<Utc>, DateTime<Utc>)>) -> Result<ErrorAnalysis, ErrorTrackingError> {
        let state = self.state.read().await;
        
        let (period_start, period_end) = period.unwrap_or_else(|| {
            let end = Utc::now();
            let start = end - chrono::Duration::days(7);
            (start, end)
        });

        let filtered_errors: Vec<_> = state.errors
            .iter()
            .filter(|e| e.timestamp >= period_start && e.timestamp <= period_end)
            .collect();

        let total_errors = filtered_errors.len() as u32;
        let time_span = (period_end - period_start).num_seconds() as f64;
        let error_rate = if time_span > 0.0 {
            total_errors as f64 / time_span
        } else {
            0.0
        };

        let mut severity_breakdown = HashMap::new();
        let mut category_breakdown = HashMap::new();

        for error in &filtered_errors {
            *severity_breakdown.entry(error.severity.clone()).or_insert(0) += 1;
            *category_breakdown.entry(error.category.clone()).or_insert(0) += 1;
        }

        let mut error_types: HashMap<String, (u32, DateTime<Utc>)> = HashMap::new();
        for error in &filtered_errors {
            let entry = error_types.entry(error.error_type.clone()).or_insert((0, error.timestamp));
            entry.0 += 1;
            if error.timestamp > entry.1 {
                entry.1 = error.timestamp;
            }
        }

        let mut top_errors: Vec<_> = error_types
            .into_iter()
            .map(|(error_type, (count, last_occurrence))| ErrorSummary {
                error_type,
                count,
                last_occurrence,
                impact_score: calculate_impact_score(count, last_occurrence, period_end),
            })
            .collect();

        top_errors.sort_by(|a, b| b.impact_score.partial_cmp(&a.impact_score).unwrap());
        top_errors.truncate(10);

        Ok(ErrorAnalysis {
            period_start,
            period_end,
            total_errors,
            error_rate,
            severity_breakdown,
            category_breakdown,
            top_errors,
        })
    }
}

fn calculate_impact_score(count: u32, last_occurrence: DateTime<Utc>, now: DateTime<Utc>) -> f64 {
    let recency = 1.0 - (now - last_occurrence).num_hours() as f64 / 168.0; // Week in hours
    let frequency = (count as f64).log10();
    (recency * 0.7 + frequency * 0.3).max(0.0)
}

impl ErrorTrackingMetrics {
    fn new() -> Self {
        Self {
            total_errors: prometheus::IntCounter::new(
                "error_tracking_total_errors",
                "Total number of errors tracked"
            ).unwrap(),
            errors_by_severity: prometheus::IntCounterVec::new(
                prometheus::Opts::new(
                    "error_tracking_errors_by_severity",
                    "Number of errors by severity level"
                ),
                &["severity"]
            ).unwrap(),
            errors_by_category: prometheus::IntCounterVec::new(
                prometheus::Opts::new(
                    "error_tracking_errors_by_category",
                    "Number of errors by category"
                ),
                &["category"]
            ).unwrap(),
            active_alerts: prometheus::Gauge::new(
                "error_tracking_active_alerts",
                "Number of active error alerts"
            ).unwrap(),
            error_processing_time: prometheus::Histogram::new(
                "error_tracking_processing_time",
                "Time taken to process errors"
            ).unwrap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_error_tracking() {
        let mut manager = ErrorTrackingManager::new(ErrorTrackingConfig::default());

        let error = ErrorRecord {
            id: "error-1".to_string(),
            timestamp: Utc::now(),
            error_type: "parsing".to_string(),
            message: "Failed to parse PDF".to_string(),
            stack_trace: Some("stack trace...".to_string()),
            severity: "error".to_string(),
            category: "parsing".to_string(),
            context: ErrorContext {
                file_path: Some("/path/to/file.pdf".to_string()),
                line_number: Some(42),
                component: Some("PDFParser".to_string()),
                environment: "production".to_string(),
                metadata: HashMap::new(),
            },
            user_id: Some("user-1".to_string()),
            resolved: false,
        };

        assert!(manager.track_error(error.clone()).await.is_ok());
        
        let retrieved = manager.get_error("error-1").await.unwrap();
        assert_eq!(retrieved.message, "Failed to parse PDF");

        let analysis = manager.analyze_errors(None).await.unwrap();
        assert_eq!(analysis.total_errors, 1);
    }
}
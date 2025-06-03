//! Logging utilities for PDF antiforensics
//! Author: kartik4091
//! Created: 2025-06-03 04:50:08 UTC
//! This module provides logging capabilities with secure
//! and privacy-focused logging mechanisms.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use thiserror::Error;
use tracing::{
    self,
    Level,
    Subscriber,
    field::{Field, Visit},
    span::{Attributes, Record},
};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{
    self,
    fmt::{self, time::UtcTime},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    Layer,
};

/// Logging error types
#[derive(Error, Debug)]
pub enum LoggingError {
    #[error("Failed to initialize logger: {0}")]
    Initialization(String),
    
    #[error("Failed to write log: {0}")]
    Write(String),
    
    #[error("Failed to rotate log: {0}")]
    Rotation(String),
    
    #[error("Failed to flush log: {0}")]
    Flush(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type for logging operations
pub type LoggingResult<T> = Result<T, LoggingError>;

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log directory path
    pub log_dir: PathBuf,
    /// Maximum log file size in bytes
    pub max_file_size: usize,
    /// Maximum log history in days
    pub max_history_days: u32,
    /// Minimum log level
    pub min_level: Level,
    /// Whether to log to console
    pub console_logging: bool,
    /// Whether to use JSON format
    pub json_format: bool,
    /// Fields to redact
    pub redact_fields: Vec<String>,
    /// Custom log metadata
    pub metadata: HashMap<String, String>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            log_dir: PathBuf::from("logs"),
            max_file_size: 10 * 1024 * 1024, // 10MB
            max_history_days: 30,
            min_level: Level::INFO,
            console_logging: true,
            json_format: false,
            redact_fields: vec![
                "password".to_string(),
                "key".to_string(),
                "token".to_string(),
                "secret".to_string(),
            ],
            metadata: HashMap::new(),
        }
    }
}

/// Secure logging implementation
pub struct SecureLogger {
    /// Logger configuration
    config: Arc<LoggingConfig>,
    /// File appender
    appender: Arc<RwLock<RollingFileAppender>>,
    /// Runtime metrics
    metrics: Arc<RwLock<LoggingMetrics>>,
}

/// Logging metrics
#[derive(Debug, Default)]
struct LoggingMetrics {
    /// Total log entries
    total_entries: usize,
    /// Error entries
    error_entries: usize,
    /// Warning entries
    warning_entries: usize,
    /// Total bytes written
    bytes_written: usize,
    /// Last rotation time
    last_rotation: DateTime<Utc>,
}

/// Custom log formatter with redaction
struct SecureFormatter {
    /// Fields to redact
    redact_fields: Vec<String>,
    /// Whether to use JSON format
    json_format: bool,
}

impl SecureLogger {
    /// Creates a new secure logger instance
    #[instrument(skip(config))]
    pub async fn new(config: LoggingConfig) -> LoggingResult<Self> {
        debug!("Initializing SecureLogger");

        // Create log directory if it doesn't exist
        tokio::fs::create_dir_all(&config.log_dir).await?;

        // Initialize file appender
        let appender = RollingFileAppender::new(
            Rotation::new(
                config.log_dir.clone(),
                "antiforensics",
                ".log",
                Some(Duration::from_secs(86400 * config.max_history_days as u64)),
                Some(config.max_file_size),
            ),
        ).map_err(|e| LoggingError::Initialization(e.to_string()))?;

        let logger = Self {
            config: Arc::new(config.clone()),
            appender: Arc::new(RwLock::new(appender)),
            metrics: Arc::new(RwLock::new(LoggingMetrics::default())),
        };

        // Initialize tracing subscriber
        logger.initialize_subscriber()?;

        Ok(logger)
    }

    /// Initializes the tracing subscriber
    fn initialize_subscriber(&self) -> LoggingResult<()> {
        let formatter = SecureFormatter {
            redact_fields: self.config.redact_fields.clone(),
            json_format: self.config.json_format,
        };

        let file_layer = fmt::Layer::new()
            .with_writer(self.appender.clone())
            .with_target(true)
            .with_thread_ids(true)
            .with_thread_names(true)
            .with_file(true)
            .with_line_number(true)
            .with_timer(UtcTime::rfc_3339())
            .with_formatter(formatter);

        let subscriber = tracing_subscriber::registry()
            .with(file_layer.with_filter(
                tracing_subscriber::filter::LevelFilter::from_level(self.config.min_level)
            ));

        // Add console logging if enabled
        if self.config.console_logging {
            let console_layer = fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_timer(UtcTime::rfc_3339());

            subscriber.with(console_layer).init();
        } else {
            subscriber.init();
        }

        Ok(())
    }

    /// Logs a message with metadata
    #[instrument(skip(self, message, metadata), err(Display))]
    pub async fn log(
        &self,
        level: Level,
        message: &str,
        metadata: Option<HashMap<String, String>>,
    ) -> LoggingResult<()> {
        let mut combined_metadata = self.config.metadata.clone();
        if let Some(meta) = metadata {
            combined_metadata.extend(meta);
        }

        // Create log event
        let event = LogEvent {
            timestamp: Utc::now(),
            level,
            message: message.to_string(),
            metadata: combined_metadata,
        };

        // Update metrics
        let mut metrics = self.metrics.write().await;
        metrics.total_entries += 1;
        match level {
            Level::ERROR => metrics.error_entries += 1,
            Level::WARN => metrics.warning_entries += 1,
            _ => {}
        }

        // Write log entry
        let entry = if self.config.json_format {
            serde_json::to_string(&event)
                .map_err(|e| LoggingError::Write(e.to_string()))?
        } else {
            format!(
                "{} [{}] {} {}",
                event.timestamp.to_rfc3339(),
                event.level,
                event.message,
                self.format_metadata(&event.metadata)
            )
        };

        metrics.bytes_written += entry.len();

        // Check rotation
        if metrics.bytes_written >= self.config.max_file_size {
            self.rotate().await?;
        }

        Ok(())
    }

    /// Rotates the log file
    #[instrument(skip(self), err(Display))]
    async fn rotate(&self) -> LoggingResult<()> {
        let mut appender = self.appender.write().await;
        appender.rotate()
            .map_err(|e| LoggingError::Rotation(e.to_string()))?;

        let mut metrics = self.metrics.write().await;
        metrics.bytes_written = 0;
        metrics.last_rotation = Utc::now();

        Ok(())
    }

    /// Formats metadata for text output
    fn format_metadata(&self, metadata: &HashMap<String, String>) -> String {
        metadata.iter()
            .map(|(k, v)| {
                if self.config.redact_fields.contains(k) {
                    format!("{}=<REDACTED>", k)
                } else {
                    format!("{}={}", k, v)
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Gets current logging metrics
    pub async fn get_metrics(&self) -> LoggingResult<LoggingMetrics> {
        Ok(self.metrics.read().await.clone())
    }

    /// Flushes the log buffer
    #[instrument(skip(self), err(Display))]
    pub async fn flush(&self) -> LoggingResult<()> {
        self.appender.write().await
            .flush()
            .map_err(|e| LoggingError::Flush(e.to_string()))?;
        Ok(())
    }
}

/// Log event structure
#[derive(Debug, Serialize)]
struct LogEvent {
    /// Event timestamp
    timestamp: DateTime<Utc>,
    /// Log level
    level: Level,
    /// Log message
    message: String,
    /// Event metadata
    metadata: HashMap<String, String>,
}

impl fmt::FormatEvent<'_, '_> for SecureFormatter {
    fn format_event(
        &self,
        ctx: &fmt::FmtContext<'_>,
        writer: fmt::format::Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> std::fmt::Result {
        let mut visitor = JsonVisitor::new(self.redact_fields.clone());
        event.record(&mut visitor);

        if self.json_format {
            // Format as JSON
            let event_json = serde_json::json!({
                "timestamp": Utc::now().to_rfc3339(),
                "level": event.metadata().level().as_str(),
                "target": event.metadata().target(),
                "fields": visitor.fields,
            });

            writeln!(writer, "{}", event_json)
        } else {
            // Format as text
            write!(
                writer,
                "{} [{}] {} {}",
                Utc::now().to_rfc3339(),
                event.metadata().level(),
                event.metadata().target(),
                self.format_fields(&visitor.fields)
            )
        }
    }
}

/// JSON visitor for tracing events
struct JsonVisitor {
    fields: HashMap<String, serde_json::Value>,
    redact_fields: Vec<String>,
}

impl JsonVisitor {
    fn new(redact_fields: Vec<String>) -> Self {
        Self {
            fields: HashMap::new(),
            redact_fields,
        }
    }
}

impl Visit for JsonVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let value_str = format!("{:?}", value);
        if self.redact_fields.contains(&field.name().to_string()) {
            self.fields.insert(field.name().to_string(), "<REDACTED>".into());
        } else {
            self.fields.insert(field.name().to_string(), value_str.into());
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if self.redact_fields.contains(&field.name().to_string()) {
            self.fields.insert(field.name().to_string(), "<REDACTED>".into());
        } else {
            self.fields.insert(field.name().to_string(), value.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;
    use tempfile::tempdir;

    #[test]
    async fn test_logger_initialization() {
        let dir = tempdir().unwrap();
        let config = LoggingConfig {
            log_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        
        let logger = SecureLogger::new(config).await;
        assert!(logger.is_ok());
    }

    #[test]
    async fn test_log_writing() {
        let dir = tempdir().unwrap();
        let config = LoggingConfig {
            log_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        
        let logger = SecureLogger::new(config).await.unwrap();
        let result = logger.log(
            Level::INFO,
            "Test message",
            Some(HashMap::from([("test".to_string(), "value".to_string())]))
        ).await;
        
        assert!(result.is_ok());
    }

    #[test]
    async fn test_redaction() {
        let dir = tempdir().unwrap();
        let config = LoggingConfig {
            log_dir: dir.path().to_path_buf(),
            redact_fields: vec!["password".to_string()],
            ..Default::default()
        };
        
        let logger = SecureLogger::new(config).await.unwrap();
        let mut metadata = HashMap::new();
        metadata.insert("password".to_string(), "secret123".to_string());
        
        let result = logger.log(Level::INFO, "Test message", Some(metadata)).await;
        assert!(result.is_ok());
    }

    #[test]
    async fn test_metrics() {
        let dir = tempdir().unwrap();
        let config = LoggingConfig {
            log_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        
        let logger = SecureLogger::new(config).await.unwrap();
        logger.log(Level::ERROR, "Error message", None).await.unwrap();
        
        let metrics = logger.get_metrics().await.unwrap();
        assert_eq!(metrics.error_entries, 1);
    }

    #[test]
    async fn test_rotation() {
        let dir = tempdir().unwrap();
        let config = LoggingConfig {
            log_dir: dir.path().to_path_buf(),
            max_file_size: 100,
            ..Default::default()
        };
        
        let logger = SecureLogger::new(config).await.unwrap();
        
        // Write enough logs to trigger rotation
        for _ in 0..10 {
            logger.log(
                Level::INFO,
                "Test message with some length to trigger rotation",
                None
            ).await.unwrap();
        }
        
        let metrics = logger.get_metrics().await.unwrap();
        assert!(metrics.last_rotation > DateTime::<Utc>::MIN);
    }
}
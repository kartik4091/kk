//! Logging Implementation
//! Author: kartik4091
//! Created: 2025-06-03 09:20:38 UTC

use super::*;
use std::{
    sync::Arc,
    path::PathBuf,
    time::{Duration, Instant},
    collections::{HashMap, VecDeque},
};
use tokio::{
    sync::{RwLock, broadcast},
    fs::{self, File, OpenOptions},
    io::AsyncWriteExt,
};
use tracing::{info, warn, error, debug, instrument};
use chrono::{DateTime, Utc};

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// Log file path
    pub log_file: PathBuf,
    /// Maximum log size
    pub max_size: u64,
    /// Maximum log files
    pub max_files: usize,
    /// Log rotation interval
    pub rotation_interval: Duration,
    /// Log level
    pub level: LogLevel,
    /// Enable console output
    pub console_output: bool,
    /// Enable timestamps
    pub timestamps: bool,
    /// Enable metrics
    pub enable_metrics: bool,
}

/// Log level
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// Log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Level
    pub level: LogLevel,
    /// Message
    pub message: String,
    /// Module
    pub module: String,
    /// File
    pub file: String,
    /// Line
    pub line: u32,
    /// Thread ID
    pub thread_id: u64,
    /// Additional fields
    pub fields: HashMap<String, String>,
}

/// Logger statistics
#[derive(Debug, Clone, Default)]
pub struct LogStats {
    /// Total entries
    pub total_entries: u64,
    /// Entries by level
    pub entries_by_level: HashMap<LogLevel, u64>,
    /// Current file size
    pub current_size: u64,
    /// Total files
    pub total_files: usize,
}

pub struct Logger {
    /// Logger configuration
    config: Arc<LogConfig>,
    /// Current log file
    file: Arc<RwLock<Option<File>>>,
    /// Log entry buffer
    buffer: Arc<RwLock<VecDeque<LogEntry>>>,
    /// Statistics
    stats: Arc<RwLock<LogStats>>,
    /// Log broadcast channel
    broadcast_tx: broadcast::Sender<LogEntry>,
    /// Metrics
    metrics: Arc<Metrics>,
}

impl Logger {
    /// Creates a new logger instance
    pub async fn new(config: LogConfig) -> Result<Self> {
        // Create log directory if it doesn't exist
        if let Some(parent) = config.log_file.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Create broadcast channel
        let (broadcast_tx, _) = broadcast::channel(1000);

        let logger = Self {
            config: Arc::new(config),
            file: Arc::new(RwLock::new(None)),
            buffer: Arc::new(RwLock::new(VecDeque::new())),
            stats: Arc::new(RwLock::new(LogStats::default())),
            broadcast_tx,
            metrics: Arc::new(Metrics::new()),
        };

        // Initialize log file
        logger.rotate_log().await?;

        // Start rotation task
        let logger_clone = logger.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(logger_clone.config.rotation_interval).await;
                if let Err(e) = logger_clone.rotate_log().await {
                    eprintln!("Log rotation error: {}", e);
                }
            }
        });

        Ok(logger)
    }

    /// Logs a message
    #[instrument(skip(self))]
    pub async fn log(&self, level: LogLevel, message: &str, module: &str, file: &str, line: u32) -> Result<()> {
        if level as u8 > self.config.level as u8 {
            return Ok(());
        }

        let start = Instant::now();
        let entry = LogEntry {
            timestamp: Utc::now(),
            level,
            message: message.to_string(),
            module: module.to_string(),
            file: file.to_string(),
            line,
            thread_id: std::thread::current().id().as_u64().unwrap_or(0),
            fields: HashMap::new(),
        };

        // Add to buffer
        let mut buffer = self.buffer.write().await;
        buffer.push_back(entry.clone());

        // Update statistics
        let mut stats = self.stats.write().await;
        stats.total_entries += 1;
        *stats.entries_by_level.entry(level).or_insert(0) += 1;

        // Write to file
        if let Some(file) = self.file.write().await.as_mut() {
            let log_line = self.format_entry(&entry);
            file.write_all(log_line.as_bytes()).await?;
            file.write_all(b"\n").await?;
            stats.current_size += log_line.len() as u64 + 1;

            // Check if rotation needed
            if stats.current_size >= self.config.max_size {
                drop(stats);
                self.rotate_log().await?;
            }
        }

        // Write to console if enabled
        if self.config.console_output {
            let formatted = self.format_entry(&entry);
            match level {
                LogLevel::Error => eprintln!("{}", formatted),
                LogLevel::Warn => println!("\x1b[33m{}\x1b[0m", formatted),
                LogLevel::Info => println!("{}", formatted),
                LogLevel::Debug => println!("\x1b[36m{}\x1b[0m", formatted),
                LogLevel::Trace => println!("\x1b[90m{}\x1b[0m", formatted),
            }
        }

        // Broadcast entry
        let _ = self.broadcast_tx.send(entry);

        // Record metrics
        if self.config.enable_metrics {
            self.metrics.record_operation("log_write", start.elapsed()).await?;
        }

        Ok(())
    }

    /// Rotates log file
    #[instrument(skip(self))]
    async fn rotate_log(&self) -> Result<()> {
        let start = Instant::now();

        // Close current file
        let mut current_file = self.file.write().await;
        *current_file = None;

        // Rename existing log files
        let base_path = self.config.log_file.with_extension("");
        for i in (1..self.config.max_files).rev() {
            let src = base_path.with_extension(format!("log.{}", i));
            let dst = base_path.with_extension(format!("log.{}", i + 1));
            if src.exists() {
                fs::rename(&src, &dst).await?;
            }
        }

        // Rename current log file
        if self.config.log_file.exists() {
            fs::rename(
                &self.config.log_file,
                base_path.with_extension("log.1"),
            ).await?;
        }

        // Create new log file
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(true)
            .open(&self.config.log_file)
            .await?;

        *current_file = Some(file);

        // Reset current size
        let mut stats = self.stats.write().await;
        stats.current_size = 0;

        // Record metrics
        if self.config.enable_metrics {
            self.metrics.record_operation("log_rotation", start.elapsed()).await?;
        }

        Ok(())
    }

    /// Formats a log entry
    fn format_entry(&self, entry: &LogEntry) -> String {
        let mut formatted = String::new();

        if self.config.timestamps {
            formatted.push_str(&format!("[{}] ", entry.timestamp.format("%Y-%m-%d %H:%M:%S%.3f")));
        }

        formatted.push_str(&format!(
            "{:<5} [{}:{}] {} - {}",
            format!("{:?}", entry.level),
            entry.file,
            entry.line,
            entry.module,
            entry.message
        ));

        if !entry.fields.is_empty() {
            formatted.push_str(" {");
            for (k, v) in &entry.fields {
                formatted.push_str(&format!(" {}={}", k, v));
            }
            formatted.push_str(" }");
        }

        formatted
    }

    /// Gets a log entry subscriber
    pub fn subscribe(&self) -> broadcast::Receiver<LogEntry> {
        self.broadcast_tx.subscribe()
    }

    /// Gets logger statistics
    pub async fn get_stats(&self) -> LogStats {
        self.stats.read().await.clone()
    }

    /// Flushes log buffer
    pub async fn flush(&self) -> Result<()> {
        let mut buffer = self.buffer.write().await;
        if let Some(file) = self.file.write().await.as_mut() {
            while let Some(entry) = buffer.pop_front() {
                let log_line = self.format_entry(&entry);
                file.write_all(log_line.as_bytes()).await?;
                file.write_all(b"\n").await?;
            }
            file.flush().await?;
        }
        Ok(())
    }
}

impl Clone for Logger {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            file: self.file.clone(),
            buffer: self.buffer.clone(),
            stats: self.stats.clone(),
            broadcast_tx: self.broadcast_tx.clone(),
            metrics: self.metrics.clone(),
        }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            log_file: PathBuf::from("logs/app.log"),
            max_size: 10 * 1024 * 1024, // 10MB
            max_files: 5,
            rotation_interval: Duration::from_secs(3600), // 1 hour
            level: LogLevel::Info,
            console_output: true,
            timestamps: true,
            enable_metrics: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    async fn create_test_logger() -> Logger {
        let temp_dir = tempdir().unwrap();
        let config = LogConfig {
            log_file: temp_dir.path().join("test.log"),
            max_size: 1024,
            max_files: 3,
            rotation_interval: Duration::from_millis(100),
            ..LogConfig::default()
        };
        Logger::new(config).await.unwrap()
    }

    #[tokio::test]
    async fn test_log_writing() {
        let logger = create_test_logger().await;
        
        logger.log(LogLevel::Info, "Test message", "test", "test.rs", 1).await.unwrap();
        
        let stats = logger.get_stats().await;
        assert_eq!(stats.total_entries, 1);
        assert_eq!(*stats.entries_by_level.get(&LogLevel::Info).unwrap(), 1);
    }

    #[tokio::test]
    async fn test_log_rotation() {
        let logger = create_test_logger().await;
        
        // Write enough logs to trigger rotation
        for i in 0..100 {
            logger.log(
                LogLevel::Info,
                &format!("Test message {}", i),
                "test",
                "test.rs",
                1
            ).await.unwrap();
        }
        
        // Wait for rotation
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        // Check that backup files exist
        let base_path = logger.config.log_file.with_extension("");
        assert!(base_path.with_extension("log.1").exists());
    }

    #[tokio::test]
    async fn test_log_levels() {
        let logger = create_test_logger().await;
        
        logger.log(LogLevel::Debug, "Debug message", "test", "test.rs", 1).await.unwrap();
        logger.log(LogLevel::Info, "Info message", "test", "test.rs", 1).await.unwrap();
        
        let stats = logger.get_stats().await;
        assert_eq!(*stats.entries_by_level.get(&LogLevel::Info).unwrap(), 1);
        assert_eq!(*stats.entries_by_level.get(&LogLevel::Debug).unwrap_or(&0), 0);
    }

    #[tokio::test]
    async fn test_log_subscription() {
        let logger = create_test_logger().await;
        let mut subscriber = logger.subscribe();
        
        logger.log(LogLevel::Info, "Test message", "test", "test.rs", 1).await.unwrap();
        
        let received = subscriber.recv().await.unwrap();
        assert_eq!(received.message, "Test message");
        assert_eq!(received.level, LogLevel::Info);
    }

    #[tokio::test]
    async fn test_log_flush() {
        let logger = create_test_logger().await;
        
        logger.log(LogLevel::Info, "Test message", "test", "test.rs", 1).await.unwrap();
        logger.flush().await.unwrap();
        
        let content = fs::read_to_string(&logger.config.log_file).await.unwrap();
        assert!(content.contains("Test message"));
    }

    #[tokio::test]
    async fn test_concurrent_logging() {
        let logger = create_test_logger().await;
        let logger_clone = logger.clone();
        
        let handle1 = tokio::spawn(async move {
            for i in 0..100 {
                logger.log(
                    LogLevel::Info,
                    &format!("Message {}", i),
                    "test1",
                    "test.rs",
                    1
                ).await.unwrap();
            }
        });
        
        let handle2 = tokio::spawn(async move {
            for i in 0..100 {
                logger_clone.log(
                    LogLevel::Info,
                    &format!("Message {}", i),
                    "test2",
                    "test.rs",
                    1
                ).await.unwrap();
            }
        });
        
        handle1.await.unwrap();
        handle2.await.unwrap();
        
        let stats = logger.get_stats().await;
        assert_eq!(stats.total_entries, 200);
    }
    }

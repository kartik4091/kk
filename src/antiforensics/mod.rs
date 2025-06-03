//! Core module for antiforensics library
//! Created: 2025-06-03 11:19:28 UTC
//! Author: kartik4091

use std::{
    sync::Arc,
    time::Instant,
};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

// Module declarations
pub mod analyzer;
pub mod cleaner;
pub mod encryption;
pub mod hash;
pub mod report;
pub mod scanner;
pub mod stego;
pub mod utils;
pub mod verification;
pub mod verifier;

// Core types and utilities
pub mod types;
pub mod error;
pub mod config;

// Constants and configuration
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MAX_CONCURRENT_OPERATIONS: usize = 32;
pub const DEFAULT_BUFFER_SIZE: usize = 8192;
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

// Global state management
#[derive(Debug)]
pub struct AntiForensics {
    state: Arc<RwLock<State>>,
    config: Arc<config::Config>,
    metrics: Arc<RwLock<Metrics>>,
}

#[derive(Debug)]
struct State {
    initialized: bool,
    operation_count: u64,
    active_operations: usize,
    last_error: Option<error::Error>,
    start_time: Instant,
}

#[derive(Debug)]
struct Metrics {
    operations: HashMap<String, u64>,
    errors: HashMap<String, u64>,
    timings: HashMap<String, Vec<f64>>,
    resource_usage: ResourceUsage,
}

#[derive(Debug)]
struct ResourceUsage {
    memory_bytes: u64,
    disk_bytes: u64,
    cpu_usage: f64,
}

impl AntiForensics {
    /// Creates a new instance with the given configuration
    /// 
    /// # Arguments
    /// * `config` - Configuration for the antiforensics system
    /// 
    /// # Returns
    /// * `Result<Self>` - New instance or error if initialization fails
    /// 
    /// # Examples
    /// ```
    /// use antiforensics::{AntiForensics, config::Config};
    /// 
    /// let config = Config::default();
    /// let af = AntiForensics::new(config)?;
    /// ```
    pub async fn new(config: config::Config) -> Result<Self, error::Error> {
        let state = Arc::new(RwLock::new(State {
            initialized: false,
            operation_count: 0,
            active_operations: 0,
            last_error: None,
            start_time: Instant::now(),
        }));

        let metrics = Arc::new(RwLock::new(Metrics {
            operations: HashMap::new(),
            errors: HashMap::new(),
            timings: HashMap::new(),
            resource_usage: ResourceUsage {
                memory_bytes: 0,
                disk_bytes: 0,
                cpu_usage: 0.0,
            },
        }));

        let instance = Self {
            state: state.clone(),
            config: Arc::new(config),
            metrics: metrics.clone(),
        };

        // Initialize subsystems
        instance.initialize().await?;

        Ok(instance)
    }

    /// Initialize the antiforensics system
    async fn initialize(&self) -> Result<(), error::Error> {
        let mut state = self.state.write().await;
        if state.initialized {
            return Ok(());
        }

        debug!("Initializing antiforensics system");
        
        // Initialize subsystems
        self.init_logging()?;
        self.init_monitoring()?;
        self.init_resource_pools().await?;
        
        state.initialized = true;
        info!("Antiforensics system initialized");
        Ok(())
    }

    // Private helper functions
    fn init_logging(&self) -> Result<(), error::Error> {
        // Initialize logging based on config
        tracing_subscriber::fmt()
            .with_max_level(self.config.log_level)
            .with_target(false)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .init();
        Ok(())
    }

    fn init_monitoring(&self) -> Result<(), error::Error> {
        // Set up metrics collection
        metrics::init()?;
        Ok(())
    }

    async fn init_resource_pools(&self) -> Result<(), error::Error> {
        // Initialize connection pools, buffers etc
        Ok(())
    }

    /// Updates metrics for an operation
    async fn track_operation(&self, name: &str, duration: f64) {
        let mut metrics = self.metrics.write().await;
        
        metrics.operations
            .entry(name.to_string())
            .and_modify(|c| *c += 1)
            .or_insert(1);

        metrics.timings
            .entry(name.to_string())
            .and_modify(|v| v.push(duration))
            .or_insert_with(|| vec![duration]);
    }

    /// Updates error metrics
    async fn track_error(&self, error: &error::Error) {
        let mut metrics = self.metrics.write().await;
        
        metrics.errors
            .entry(error.to_string())
            .and_modify(|c| *c += 1)
            .or_insert(1);
    }
}

// Test module
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn test_new_instance() {
        let rt = Runtime::new().unwrap();
        
        rt.block_on(async {
            let config = config::Config::default();
            let af = AntiForensics::new(config).await.unwrap();
            
            let state = af.state.read().await;
            assert!(state.initialized);
            assert_eq!(state.operation_count, 0);
            assert_eq!(state.active_operations, 0);
            assert!(state.last_error.is_none());
        });
    }

    #[test]
    fn test_metrics_tracking() {
        let rt = Runtime::new().unwrap();
        
        rt.block_on(async {
            let config = config::Config::default();
            let af = AntiForensics::new(config).await.unwrap();
            
            af.track_operation("test_op", 1.5).await;
            
            let metrics = af.metrics.read().await;
            assert_eq!(metrics.operations.get("test_op"), Some(&1));
            assert_eq!(metrics.timings.get("test_op"), Some(&vec![1.5]));
        });
    }

    #[test]
    fn test_error_tracking() {
        let rt = Runtime::new().unwrap();
        
        rt.block_on(async {
            let config = config::Config::default();
            let af = AntiForensics::new(config).await.unwrap();
            
            let error = error::Error::InitializationError("test".into());
            af.track_error(&error).await;
            
            let metrics = af.metrics.read().await;
            assert_eq!(metrics.errors.get(&error.to_string()), Some(&1));
        });
    }
}

// Re-exports for convenience
pub use analyzer::AnalyzerResult;
pub use cleaner::CleanerResult;
pub use types::{ProcessingMetrics, RiskLevel};
pub use error::{Error, Result};

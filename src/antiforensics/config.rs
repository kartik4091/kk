//! Configuration management for antiforensics system
//! Created: 2025-06-03 12:13:36 UTC
//! Author: kartik4091

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn, Level};

use crate::error::{Error, Result};

/// Core configuration structure for the antiforensics system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// General settings
    pub general: GeneralConfig,
    
    /// Performance settings
    pub performance: PerformanceConfig,
    
    /// Security settings
    pub security: SecurityConfig,
    
    /// Analysis settings
    pub analysis: AnalysisConfig,
    
    /// Cleaning settings
    pub cleaning: CleaningConfig,
    
    /// Scanner settings
    pub scanner: ScannerConfig,
    
    /// Logging configuration
    pub logging: LoggingConfig,
    
    /// Resource limits
    pub resources: ResourceConfig,
    
    /// Custom settings
    #[serde(default)]
    pub custom: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub workspace_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub max_file_size: u64,
    pub default_timeout: Duration,
    pub enable_metrics: bool,
    pub enable_tracing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    pub thread_pool_size: usize,
    pub max_concurrent_tasks: usize,
    pub buffer_size: usize,
    pub cache_size_mb: u64,
    pub batch_size: usize,
    pub io_buffer_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub max_memory_mb: u64,
    pub max_disk_usage_mb: u64,
    pub allowed_file_types: Vec<String>,
    pub blocked_file_types: Vec<String>,
    pub encryption_algorithm: String,
    pub key_size: u32,
    pub enable_sandbox: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    pub deep_scan: bool,
    pub risk_threshold: f64,
    pub max_analysis_time: Duration,
    pub patterns_file: PathBuf,
    pub enable_ml: bool,
    pub ml_model_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleaningConfig {
    pub backup_files: bool,
    pub backup_dir: PathBuf,
    pub secure_delete: bool,
    pub wipe_passes: u32,
    pub preserve_metadata: Vec<String>,
    pub cleaning_rules: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerConfig {
    pub scan_depth: u32,
    pub follow_symlinks: bool,
    pub exclude_dirs: Vec<PathBuf>,
    pub scan_timeout: Duration,
    pub signature_db: PathBuf,
    pub enable_yara: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub log_level: Level,
    pub log_file: PathBuf,
    pub max_log_size: u64,
    pub max_log_files: u32,
    pub log_format: String,
    pub enable_syslog: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConfig {
    pub max_cpu_percent: f64,
    pub max_memory_percent: f64,
    pub max_disk_percent: f64,
    pub io_priority: u8,
    pub nice_value: i8,
}

/// Configuration manager for dynamic config updates
#[derive(Debug)]
pub struct ConfigManager {
    config: Arc<RwLock<Config>>,
    config_path: PathBuf,
    watchers: Vec<ConfigWatcher>,
}

type ConfigWatcher = Box<dyn Fn(&Config) -> Result<()> + Send + Sync>;

impl Config {
    pub fn default() -> Self {
        Self {
            general: GeneralConfig {
                workspace_dir: PathBuf::from("/tmp/antiforensics"),
                temp_dir: std::env::temp_dir(),
                max_file_size: 1024 * 1024 * 100,
                default_timeout: Duration::from_secs(30),
                enable_metrics: true,
                enable_tracing: true,
            },
            performance: PerformanceConfig {
                thread_pool_size: num_cpus::get(),
                max_concurrent_tasks: 32,
                buffer_size: 8192,
                cache_size_mb: 1024,
                batch_size: 1000,
                io_buffer_size: 65536,
            },
            security: SecurityConfig {
                max_memory_mb: 1024,
                max_disk_usage_mb: 10240,
                allowed_file_types: vec!["pdf".into(), "doc".into()],
                blocked_file_types: vec!["exe".into(), "dll".into()],
                encryption_algorithm: "AES-256-GCM".into(),
                key_size: 256,
                enable_sandbox: true,
            },
            analysis: AnalysisConfig {
                deep_scan: true,
                risk_threshold: 0.7,
                max_analysis_time: Duration::from_secs(300),
                patterns_file: PathBuf::from("patterns.yml"),
                enable_ml: false,
                ml_model_path: PathBuf::from("model.bin"),
            },
            cleaning: CleaningConfig {
                backup_files: true,
                backup_dir: PathBuf::from("backups"),
                secure_delete: true,
                wipe_passes: 3,
                preserve_metadata: vec!["CreationDate".into()],
                cleaning_rules: PathBuf::from("rules.yml"),
            },
            scanner: ScannerConfig {
                scan_depth: 5,
                follow_symlinks: false,
                exclude_dirs: vec![],
                scan_timeout: Duration::from_secs(3600),
                signature_db: PathBuf::from("signatures.db"),
                enable_yara: true,
            },
            logging: LoggingConfig {
                log_level: Level::INFO,
                log_file: PathBuf::from("antiforensics.log"),
                max_log_size: 1024 * 1024 * 10,
                max_log_files: 5,
                log_format: "json".into(),
                enable_syslog: false,
            },
            resources: ResourceConfig {
                max_cpu_percent: 80.0,
                max_memory_percent: 70.0,
                max_disk_percent: 90.0,
                io_priority: 4,
                nice_value: 0,
            },
            custom: HashMap::new(),
        }
    }

    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = fs::read_to_string(path)?;
        Self::from_str(&contents)
    }

    pub fn from_str(contents: &str) -> Result<Self> {
        serde_yaml::from_str(contents)
            .map_err(|e| Error::Configuration(format!("Failed to parse config: {}", e)))
    }

    pub fn validate(&self) -> Result<()> {
        self.validate_general()?;
        self.validate_performance()?;
        self.validate_security()?;
        self.validate_resources()?;
        Ok(())
    }

    fn validate_general(&self) -> Result<()> {
        if !self.general.workspace_dir.exists() {
            fs::create_dir_all(&self.general.workspace_dir)?;
        }
        Ok(())
    }

    fn validate_performance(&self) -> Result<()> {
        if self.performance.thread_pool_size == 0 {
            return Err(Error::Configuration("Thread pool size cannot be zero".into()));
        }
        Ok(())
    }

    fn validate_security(&self) -> Result<()> {
        if self.security.max_memory_mb == 0 {
            return Err(Error::Configuration("Max memory cannot be zero".into()));
        }
        Ok(())
    }

    fn validate_resources(&self) -> Result<()> {
        if self.resources.max_cpu_percent > 100.0 {
            return Err(Error::Configuration("CPU percentage cannot exceed 100".into()));
        }
        Ok(())
    }
}

impl ConfigManager {
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let config = Config::from_file(&path)?;
        config.validate()?;

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            config_path: path.as_ref().to_path_buf(),
            watchers: Vec::new(),
        })
    }

    pub fn add_watcher(&mut self, watcher: ConfigWatcher) {
        self.watchers.push(watcher);
    }

    pub async fn update(&self, new_config: Config) -> Result<()> {
        new_config.validate()?;

        for watcher in &self.watchers {
            watcher(&new_config)?;
        }

        let mut config = self.config.write().await;
        *config = new_config;

        Ok(())
    }

    pub async fn get_config(&self) -> Arc<Config> {
        Arc::new(self.config.read().await.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_from_file() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.yml");
        
        let config = Config::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        fs::write(&config_path, yaml).unwrap();

        let loaded = Config::from_file(&config_path).unwrap();
        assert_eq!(loaded.general.max_file_size, config.general.max_file_size);
    }

    #[tokio::test]
    async fn test_config_manager() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.yml");
        
        let config = Config::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        fs::write(&config_path, yaml).unwrap();

        let mut manager = ConfigManager::new(&config_path).await.unwrap();
        
        let watcher_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let watcher_called_clone = watcher_called.clone();
        
        manager.add_watcher(Box::new(move |_| {
            watcher_called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }));

        let mut new_config = Config::default();
        new_config.general.max_file_size = 200 * 1024 * 1024;
        
        manager.update(new_config).await.unwrap();
        assert!(watcher_called.load(std::sync::atomic::Ordering::SeqCst));
    }
}
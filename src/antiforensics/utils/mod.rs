//! Utility Module Implementation
//! Author: kartik4091
//! Created: 2025-06-03 09:14:13 UTC

use std::{
    sync::Arc,
    path::PathBuf,
    time::{Duration, Instant},
    collections::{HashMap, HashSet},
};
use tokio::{
    sync::{RwLock, Semaphore, broadcast},
    fs::{self, File},
    io::{AsyncReadExt, AsyncWriteExt},
};
use tracing::{info, warn, error, debug, instrument};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};

pub mod metrics;
pub mod cache;
pub mod validation;
pub mod logging;

pub use self::{
    metrics::Metrics,
    cache::Cache,
    validation::Validation,
    logging::Logger,
};

/// Error types for utility operations
#[derive(Debug, thiserror::Error)]
pub enum UtilError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("Metric error: {0}")]
    Metric(String),

    #[error("Logging error: {0}")]
    Logging(String),
}

/// Result type for utility operations
pub type Result<T> = std::result::Result<T, UtilError>;

/// Configuration trait for utilities
#[async_trait]
pub trait UtilityConfig: Send + Sync {
    /// Validates configuration
    fn validate(&self) -> Result<()>;
    
    /// Gets configuration value
    fn get(&self, key: &str) -> Option<String>;
    
    /// Sets configuration value
    fn set(&mut self, key: &str, value: String) -> Result<()>;
}

/// Base configuration implementation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseConfig {
    /// Configuration values
    values: HashMap<String, String>,
    /// Configuration schema
    schema: HashMap<String, ConfigSchema>,
}

/// Configuration schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSchema {
    /// Value type
    value_type: ConfigValueType,
    /// Required flag
    required: bool,
    /// Default value
    default: Option<String>,
    /// Validation rules
    validation: Vec<ValidationRule>,
}

/// Configuration value types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigValueType {
    String,
    Number,
    Boolean,
    Duration,
    Path,
}

/// Validation rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationRule {
    /// Minimum value
    Min(f64),
    /// Maximum value
    Max(f64),
    /// Pattern match
    Pattern(String),
    /// Enumerated values
    Enum(Vec<String>),
    /// Path exists
    PathExists,
    /// Path is file
    PathIsFile,
    /// Path is directory
    PathIsDirectory,
}

impl BaseConfig {
    /// Creates a new base configuration
    pub fn new(schema: HashMap<String, ConfigSchema>) -> Self {
        Self {
            values: HashMap::new(),
            schema,
        }
    }

    /// Validates value against schema
    fn validate_value(&self, key: &str, value: &str) -> Result<()> {
        if let Some(schema) = self.schema.get(key) {
            // Validate type
            match schema.value_type {
                ConfigValueType::Number => {
                    value.parse::<f64>().map_err(|_| {
                        UtilError::Validation(format!("Invalid number value for {}", key))
                    })?;
                }
                ConfigValueType::Boolean => {
                    value.parse::<bool>().map_err(|_| {
                        UtilError::Validation(format!("Invalid boolean value for {}", key))
                    })?;
                }
                ConfigValueType::Duration => {
                    let duration = humantime::parse_duration(value).map_err(|_| {
                        UtilError::Validation(format!("Invalid duration value for {}", key))
                    })?;
                }
                ConfigValueType::Path => {
                    let path = PathBuf::from(value);
                }
                _ => {}
            }

            // Validate rules
            for rule in &schema.validation {
                match rule {
                    ValidationRule::Min(min) => {
                        if let Ok(num) = value.parse::<f64>() {
                            if num < *min {
                                return Err(UtilError::Validation(
                                    format!("Value {} is less than minimum {}", num, min)
                                ));
                            }
                        }
                    }
                    ValidationRule::Max(max) => {
                        if let Ok(num) = value.parse::<f64>() {
                            if num > *max {
                                return Err(UtilError::Validation(
                                    format!("Value {} is greater than maximum {}", num, max)
                                ));
                            }
                        }
                    }
                    ValidationRule::Pattern(pattern) => {
                        let re = regex::Regex::new(pattern).map_err(|_| {
                            UtilError::Validation("Invalid regex pattern".into())
                        })?;
                        if !re.is_match(value) {
                            return Err(UtilError::Validation(
                                format!("Value {} does not match pattern {}", value, pattern)
                            ));
                        }
                    }
                    ValidationRule::Enum(values) => {
                        if !values.contains(&value.to_string()) {
                            return Err(UtilError::Validation(
                                format!("Value {} is not in allowed values: {:?}", value, values)
                            ));
                        }
                    }
                    ValidationRule::PathExists => {
                        let path = PathBuf::from(value);
                        if !path.exists() {
                            return Err(UtilError::Validation(
                                format!("Path does not exist: {}", value)
                            ));
                        }
                    }
                    ValidationRule::PathIsFile => {
                        let path = PathBuf::from(value);
                        if !path.is_file() {
                            return Err(UtilError::Validation(
                                format!("Path is not a file: {}", value)
                            ));
                        }
                    }
                    ValidationRule::PathIsDirectory => {
                        let path = PathBuf::from(value);
                        if !path.is_dir() {
                            return Err(UtilError::Validation(
                                format!("Path is not a directory: {}", value)
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl UtilityConfig for BaseConfig {
    fn validate(&self) -> Result<()> {
        for (key, schema) in &self.schema {
            if schema.required {
                if let Some(value) = self.values.get(key) {
                    self.validate_value(key, value)?;
                } else if schema.default.is_none() {
                    return Err(UtilError::Validation(
                        format!("Required value {} is missing", key)
                    ));
                }
            }
        }
        Ok(())
    }

    fn get(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned().or_else(|| {
            self.schema.get(key).and_then(|s| s.default.clone())
        })
    }

    fn set(&mut self, key: &str, value: String) -> Result<()> {
        self.validate_value(key, &value)?;
        self.values.insert(key.to_string(), value);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_schema() -> HashMap<String, ConfigSchema> {
        let mut schema = HashMap::new();
        
        schema.insert("number".into(), ConfigSchema {
            value_type: ConfigValueType::Number,
            required: true,
            default: None,
            validation: vec![
                ValidationRule::Min(0.0),
                ValidationRule::Max(100.0),
            ],
        });

        schema.insert("string".into(), ConfigSchema {
            value_type: ConfigValueType::String,
            required: true,
            default: Some("default".into()),
            validation: vec![
                ValidationRule::Pattern(r"^[a-z]+$".into()),
            ],
        });

        schema.insert("duration".into(), ConfigSchema {
            value_type: ConfigValueType::Duration,
            required: false,
            default: Some("1h".into()),
            validation: vec![],
        });

        schema
    }

    #[test]
    fn test_config_validation() {
        let mut config = BaseConfig::new(create_test_schema());
        
        // Valid values
        assert!(config.set("number", "50".into()).is_ok());
        assert!(config.set("string", "test".into()).is_ok());
        assert!(config.set("duration", "2h".into()).is_ok());
        
        // Invalid values
        assert!(config.set("number", "200".into()).is_err());
        assert!(config.set("string", "TEST123".into()).is_err());
        assert!(config.set("duration", "invalid".into()).is_err());
    }

    #[test]
    fn test_config_defaults() {
        let config = BaseConfig::new(create_test_schema());
        
        assert_eq!(config.get("string"), Some("default".into()));
        assert_eq!(config.get("duration"), Some("1h".into()));
        assert_eq!(config.get("number"), None);
    }

    #[test]
    fn test_required_values() {
        let config = BaseConfig::new(create_test_schema());
        
        // Missing required value
        assert!(config.validate().is_err());
        
        let mut config = BaseConfig::new(create_test_schema());
        config.set("number", "50".into()).unwrap();
        config.set("string", "test".into()).unwrap();
        
        // All required values present
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_value_types() {
        let mut config = BaseConfig::new(create_test_schema());
        
        // Number validation
        assert!(config.set("number", "not_a_number".into()).is_err());
        
        // Duration validation
        assert!(config.set("duration", "1h30m".into()).is_ok());
        assert!(config.set("duration", "invalid".into()).is_err());
    }

    #[test]
    fn test_validation_rules() {
        let mut config = BaseConfig::new(create_test_schema());
        
        // Pattern validation
        assert!(config.set("string", "test".into()).is_ok());
        assert!(config.set("string", "Test123".into()).is_err());
        
        // Range validation
        assert!(config.set("number", "0".into()).is_ok());
        assert!(config.set("number", "100".into()).is_ok());
        assert!(config.set("number", "-1".into()).is_err());
        assert!(config.set("number", "101".into()).is_err());
    }
}

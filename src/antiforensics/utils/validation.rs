//! Validation Implementation
//! Author: kartik4091
//! Created: 2025-06-03 09:19:16 UTC

use super::*;
use std::{
    sync::Arc,
    path::PathBuf,
    time::{Duration, Instant},
    collections::{HashMap, HashSet},
};
use tokio::sync::RwLock;
use regex::Regex;
use lazy_static::lazy_static;

/// Validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConfig {
    /// Maximum string length
    pub max_string_length: usize,
    /// Maximum file size
    pub max_file_size: u64,
    /// Allowed file extensions
    pub allowed_extensions: HashSet<String>,
    /// Required fields
    pub required_fields: HashSet<String>,
    /// Custom patterns
    pub patterns: HashMap<String, String>,
}

/// Validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Valid flag
    pub is_valid: bool,
    /// Validation errors
    pub errors: Vec<ValidationError>,
    /// Validation duration
    pub duration: Duration,
}

/// Validation error
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
    /// Error field
    pub field: Option<String>,
}

lazy_static! {
    static ref EMAIL_REGEX: Regex = Regex::new(
        r"^[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$"
    ).unwrap();

    static ref URL_REGEX: Regex = Regex::new(
        r"^https?://(?:www\.)?[-a-zA-Z0-9@:%._\+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b(?:[-a-zA-Z0-9()@:%_\+.~#?&/=]*)$"
    ).unwrap();

    static ref PATH_REGEX: Regex = Regex::new(
        r"^(?:/[^/]+)+/?$"
    ).unwrap();
}

pub struct Validation {
    /// Validation configuration
    config: Arc<ValidationConfig>,
    /// Custom validators
    validators: Arc<RwLock<HashMap<String, Box<dyn Validator + Send + Sync>>>>,
    /// Metrics
    metrics: Arc<Metrics>,
}

/// Validator trait
#[async_trait]
pub trait Validator {
    /// Validates a value
    async fn validate(&self, value: &str) -> Result<()>;
}

impl Validation {
    /// Creates a new validation instance
    pub fn new(config: ValidationConfig) -> Self {
        Self {
            config: Arc::new(config),
            validators: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(Metrics::new()),
        }
    }

    /// Registers a custom validator
    pub async fn register_validator<V>(&self, name: &str, validator: V) -> Result<()>
    where
        V: Validator + Send + Sync + 'static,
    {
        let mut validators = self.validators.write().await;
        validators.insert(name.to_string(), Box::new(validator));
        Ok(())
    }

    /// Validates a string value
    #[instrument(skip(self))]
    pub async fn validate_string(&self, value: &str) -> ValidationResult {
        let start = Instant::now();
        let mut errors = Vec::new();

        // Check length
        if value.len() > self.config.max_string_length {
            errors.push(ValidationError {
                code: "LENGTH_EXCEEDED".into(),
                message: format!("String length exceeds maximum of {}", self.config.max_string_length),
                field: None,
            });
        }

        let result = ValidationResult {
            is_valid: errors.is_empty(),
            errors,
            duration: start.elapsed(),
        };

        self.metrics.record_operation("string_validation", result.duration).await
            .unwrap_or_else(|e| error!("Failed to record metrics: {}", e));

        result
    }

    /// Validates a file
    #[instrument(skip(self))]
    pub async fn validate_file(&self, path: &PathBuf) -> ValidationResult {
        let start = Instant::now();
        let mut errors = Vec::new();

        // Check if file exists
        if !path.exists() {
            errors.push(ValidationError {
                code: "FILE_NOT_FOUND".into(),
                message: format!("File not found: {}", path.display()),
                field: None,
            });
            return ValidationResult {
                is_valid: false,
                errors,
                duration: start.elapsed(),
            };
        }

        // Check file size
        if let Ok(metadata) = tokio::fs::metadata(path).await {
            if metadata.len() > self.config.max_file_size {
                errors.push(ValidationError {
                    code: "SIZE_EXCEEDED".into(),
                    message: format!("File size exceeds maximum of {} bytes", self.config.max_file_size),
                    field: None,
                });
            }
        }

        // Check extension
        if let Some(ext) = path.extension() {
            if !self.config.allowed_extensions.contains(&ext.to_string_lossy().to_string()) {
                errors.push(ValidationError {
                    code: "INVALID_EXTENSION".into(),
                    message: format!("File extension not allowed: {}", ext.to_string_lossy()),
                    field: None,
                });
            }
        }

        let result = ValidationResult {
            is_valid: errors.is_empty(),
            errors,
            duration: start.elapsed(),
        };

        self.metrics.record_operation("file_validation", result.duration).await
            .unwrap_or_else(|e| error!("Failed to record metrics: {}", e));

        result
    }

    /// Validates required fields
    #[instrument(skip(self))]
    pub async fn validate_required(&self, fields: &HashMap<String, String>) -> ValidationResult {
        let start = Instant::now();
        let mut errors = Vec::new();

        for required in &self.config.required_fields {
            if !fields.contains_key(required) {
                errors.push(ValidationError {
                    code: "MISSING_FIELD".into(),
                    message: format!("Required field missing: {}", required),
                    field: Some(required.clone()),
                });
            }
        }

        let result = ValidationResult {
            is_valid: errors.is_empty(),
            errors,
            duration: start.elapsed(),
        };

        self.metrics.record_operation("required_validation", result.duration).await
            .unwrap_or_else(|e| error!("Failed to record metrics: {}", e));

        result
    }

    /// Validates against a pattern
    #[instrument(skip(self))]
    pub async fn validate_pattern(&self, name: &str, value: &str) -> ValidationResult {
        let start = Instant::now();
        let mut errors = Vec::new();

        if let Some(pattern) = self.config.patterns.get(name) {
            if let Ok(re) = Regex::new(pattern) {
                if !re.is_match(value) {
                    errors.push(ValidationError {
                        code: "PATTERN_MISMATCH".into(),
                        message: format!("Value does not match pattern: {}", name),
                        field: None,
                    });
                }
            }
        }

        let result = ValidationResult {
            is_valid: errors.is_empty(),
            errors,
            duration: start.elapsed(),
        };

        self.metrics.record_operation("pattern_validation", result.duration).await
            .unwrap_or_else(|e| error!("Failed to record metrics: {}", e));

        result
    }

    /// Validates an email address
    #[instrument(skip(self))]
    pub async fn validate_email(&self, email: &str) -> ValidationResult {
        let start = Instant::now();
        let mut errors = Vec::new();

        if !EMAIL_REGEX.is_match(email) {
            errors.push(ValidationError {
                code: "INVALID_EMAIL".into(),
                message: "Invalid email address format".into(),
                field: None,
            });
        }

        let result = ValidationResult {
            is_valid: errors.is_empty(),
            errors,
            duration: start.elapsed(),
        };

        self.metrics.record_operation("email_validation", result.duration).await
            .unwrap_or_else(|e| error!("Failed to record metrics: {}", e));

        result
    }

    /// Validates a URL
    #[instrument(skip(self))]
    pub async fn validate_url(&self, url: &str) -> ValidationResult {
        let start = Instant::now();
        let mut errors = Vec::new();

        if !URL_REGEX.is_match(url) {
            errors.push(ValidationError {
                code: "INVALID_URL".into(),
                message: "Invalid URL format".into(),
                field: None,
            });
        }

        let result = ValidationResult {
            is_valid: errors.is_empty(),
            errors,
            duration: start.elapsed(),
        };

        self.metrics.record_operation("url_validation", result.duration).await
            .unwrap_or_else(|e| error!("Failed to record metrics: {}", e));

        result
    }

    /// Validates a file path
    #[instrument(skip(self))]
    pub async fn validate_path(&self, path: &str) -> ValidationResult {
        let start = Instant::now();
        let mut errors = Vec::new();

        if !PATH_REGEX.is_match(path) {
            errors.push(ValidationError {
                code: "INVALID_PATH".into(),
                message: "Invalid path format".into(),
                field: None,
            });
        }

        let result = ValidationResult {
            is_valid: errors.is_empty(),
            errors,
            duration: start.elapsed(),
        };

        self.metrics.record_operation("path_validation", result.duration).await
            .unwrap_or_else(|e| error!("Failed to record metrics: {}", e));

        result
    }
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_string_length: 1024,
            max_file_size: 10 * 1024 * 1024, // 10MB
            allowed_extensions: ["txt", "pdf", "png", "jpg", "jpeg"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            required_fields: HashSet::new(),
            patterns: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_string_validation() {
        let validation = Validation::new(ValidationConfig {
            max_string_length: 10,
            ..ValidationConfig::default()
        });
        
        let result = validation.validate_string("short").await;
        assert!(result.is_valid);
        
        let result = validation.validate_string("very long string").await;
        assert!(!result.is_valid);
    }

    #[tokio::test]
    async fn test_file_validation() {
        let validation = Validation::new(ValidationConfig::default());
        
        let temp_dir = tempfile::tempdir().unwrap();
        let valid_path = temp_dir.path().join("test.txt");
        tokio::fs::write(&valid_path, "test").await.unwrap();
        
        let result = validation.validate_file(&valid_path).await;
        assert!(result.is_valid);
        
        let invalid_path = temp_dir.path().join("test.exe");
        let result = validation.validate_file(&invalid_path).await;
        assert!(!result.is_valid);
    }

    #[tokio::test]
    async fn test_required_fields() {
        let mut config = ValidationConfig::default();
        config.required_fields.insert("name".into());
        let validation = Validation::new(config);
        
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), "test".to_string());
        
        let result = validation.validate_required(&fields).await;
        assert!(result.is_valid);
        
        fields.clear();
        let result = validation.validate_required(&fields).await;
        assert!(!result.is_valid);
    }

    #[tokio::test]
    async fn test_pattern_validation() {
        let mut config = ValidationConfig::default();
        config.patterns.insert("digits".into(), r"^\d+$".into());
        let validation = Validation::new(config);
        
        let result = validation.validate_pattern("digits", "123").await;
        assert!(result.is_valid);
        
        let result = validation.validate_pattern("digits", "abc").await;
        assert!(!result.is_valid);
    }

    #[tokio::test]
    async fn test_email_validation() {
        let validation = Validation::new(ValidationConfig::default());
        
        let result = validation.validate_email("test@example.com").await;
        assert!(result.is_valid);
        
        let result = validation.validate_email("invalid-email").await;
        assert!(!result.is_valid);
    }

    #[tokio::test]
    async fn test_url_validation() {
        let validation = Validation::new(ValidationConfig::default());
        
        let result = validation.validate_url("https://example.com").await;
        assert!(result.is_valid);
        
        let result = validation.validate_url("invalid-url").await;
        assert!(!result.is_valid);
    }

    #[tokio::test]
    async fn test_path_validation() {
        let validation = Validation::new(ValidationConfig::default());
        
        let result = validation.validate_path("/path/to/file").await;
        assert!(result.is_valid);
        
        let result = validation.validate_path("invalid\\path").await;
        assert!(!result.is_valid);
    }

    #[tokio::test]
    async fn test_custom_validator() {
        struct EvenNumberValidator;
        
        #[async_trait]
        impl Validator for EvenNumberValidator {
            async fn validate(&self, value: &str) -> Result<()> {
                match value.parse::<i32>() {
                    Ok(n) if n % 2 == 0 => Ok(()),
                    _ => Err(UtilError::Validation("Not an even number".into())),
                }
            }
        }
        
        let validation = Validation::new(ValidationConfig::default());
        validation.register_validator("even", EvenNumberValidator).await.unwrap();
        
        let validators = validation.validators.read().await;
        let validator = validators.get("even").unwrap();
        
        assert!(validator.validate("2").await.is_ok());
        assert!(validator.validate("3").await.is_err());
    }
        }

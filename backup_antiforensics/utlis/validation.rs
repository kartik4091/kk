//! Validation utilities for PDF antiforensics
//! Author: kartik4091
//! Created: 2025-06-03 04:48:38 UTC
//! This module provides validation functions for ensuring data
//! integrity and security requirements.

use std::{
    collections::HashMap,
    path::Path,
    time::Duration,
};
use regex::Regex;
use lazy_static::lazy_static;
use serde::{Serialize, Deserialize};
use thiserror::Error;
use tracing::{info, warn, error, debug, trace, instrument};

use super::crypto::CryptoUtils;
use crate::antiforensics::{Document, PdfError};

/// Validation error types
#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Schema validation failed: {0}")]
    Schema(String),

    #[error("Content validation failed: {0}")]
    Content(String),

    #[error("Size validation failed: {0}")]
    Size(String),

    #[error("Format validation failed: {0}")]
    Format(String),

    #[error("Security validation failed: {0}")]
    Security(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type for validation operations
pub type ValidationResult<T> = Result<T, ValidationError>;

/// Validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConfig {
    /// Maximum file size in bytes
    pub max_file_size: usize,
    /// Maximum number of pages
    pub max_pages: usize,
    /// Maximum content size per page
    pub max_page_size: usize,
    /// Allowed file formats
    pub allowed_formats: Vec<String>,
    /// Required metadata fields
    pub required_metadata: Vec<String>,
    /// Content validation rules
    pub content_rules: ContentRules,
    /// Security validation rules
    pub security_rules: SecurityRules,
}

/// Content validation rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentRules {
    /// Maximum image size
    pub max_image_size: usize,
    /// Allowed image formats
    pub allowed_image_formats: Vec<String>,
    /// Maximum text length
    pub max_text_length: usize,
    /// Banned keywords
    pub banned_keywords: Vec<String>,
    /// Content patterns to validate
    pub content_patterns: HashMap<String, String>,
}

/// Security validation rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityRules {
    /// Minimum encryption key length
    pub min_key_length: usize,
    /// Required encryption algorithms
    pub required_algorithms: Vec<String>,
    /// Maximum permission level
    pub max_permission_level: u32,
    /// Required security handlers
    pub required_handlers: Vec<String>,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_file_size: 100 * 1024 * 1024, // 100MB
            max_pages: 1000,
            max_page_size: 10 * 1024 * 1024, // 10MB
            allowed_formats: vec!["pdf".to_string()],
            required_metadata: vec![
                "Creator".to_string(),
                "Producer".to_string(),
                "CreationDate".to_string(),
            ],
            content_rules: ContentRules {
                max_image_size: 5 * 1024 * 1024, // 5MB
                allowed_image_formats: vec![
                    "jpeg".to_string(),
                    "png".to_string(),
                    "tiff".to_string(),
                ],
                max_text_length: 1_000_000,
                banned_keywords: vec![
                    "javascript".to_string(),
                    "eval".to_string(),
                    "exec".to_string(),
                ],
                content_patterns: HashMap::new(),
            },
            security_rules: SecurityRules {
                min_key_length: 256,
                required_algorithms: vec![
                    "AES-256".to_string(),
                    "SHA-256".to_string(),
                ],
                max_permission_level: 3,
                required_handlers: vec![
                    "Standard".to_string(),
                ],
            },
        }
    }
}

lazy_static! {
    static ref EMAIL_REGEX: Regex = Regex::new(
        r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$"
    ).unwrap();
    
    static ref URL_REGEX: Regex = Regex::new(
        r"^https?://[^\s/$.?#].[^\s]*$"
    ).unwrap();
    
    static ref PHONE_REGEX: Regex = Regex::new(
        r"^\+?[\d\s-]{10,}$"
    ).unwrap();
}

/// Validation utilities implementation
pub struct ValidationUtils {
    /// Validation configuration
    config: ValidationConfig,
    /// Crypto utilities
    crypto: CryptoUtils,
}

impl ValidationUtils {
    /// Creates a new validation utilities instance
    #[instrument(skip(config))]
    pub fn new(config: ValidationConfig) -> Self {
        debug!("Initializing ValidationUtils");
        
        Self {
            crypto: CryptoUtils::new(Default::default()),
            config,
        }
    }

    /// Validates a PDF document
    #[instrument(skip(self, doc), err(Display))]
    pub async fn validate_document(&self, doc: &Document) -> ValidationResult<()> {
        // Validate file size
        self.validate_file_size(doc).await?;

        // Validate page count and sizes
        self.validate_pages(doc).await?;

        // Validate metadata
        self.validate_metadata(doc).await?;

        // Validate content
        self.validate_content(doc).await?;

        // Validate security settings
        self.validate_security(doc).await?;

        Ok(())
    }

    /// Validates file size
    #[instrument(skip(self, doc), err(Display))]
    async fn validate_file_size(&self, doc: &Document) -> ValidationResult<()> {
        let size = doc.get_size()?;
        if size > self.config.max_file_size {
            return Err(ValidationError::Size(format!(
                "File size {} exceeds maximum allowed size {}",
                size,
                self.config.max_file_size
            )));
        }
        Ok(())
    }

    /// Validates pages
    #[instrument(skip(self, doc), err(Display))]
    async fn validate_pages(&self, doc: &Document) -> ValidationResult<()> {
        let page_count = doc.get_page_count()?;
        if page_count > self.config.max_pages {
            return Err(ValidationError::Content(format!(
                "Page count {} exceeds maximum allowed pages {}",
                page_count,
                self.config.max_pages
            )));
        }

        for page in doc.get_pages() {
            let page_size = page.get_size()?;
            if page_size > self.config.max_page_size {
                return Err(ValidationError::Size(format!(
                    "Page size {} exceeds maximum allowed page size {}",
                    page_size,
                    self.config.max_page_size
                )));
            }
        }

        Ok(())
    }

    /// Validates metadata
    #[instrument(skip(self, doc), err(Display))]
    async fn validate_metadata(&self, doc: &Document) -> ValidationResult<()> {
        let metadata = doc.get_metadata()?;
        for field in &self.config.required_metadata {
            if !metadata.contains_key(field) {
                return Err(ValidationError::Schema(format!(
                    "Required metadata field {} is missing",
                    field
                )));
            }
        }
        Ok(())
    }

    /// Validates content
    #[instrument(skip(self, doc), err(Display))]
    async fn validate_content(&self, doc: &Document) -> ValidationResult<()> {
        // Validate images
        for image in doc.get_images()? {
            let size = image.get_size()?;
            if size > self.config.content_rules.max_image_size {
                return Err(ValidationError::Content(format!(
                    "Image size {} exceeds maximum allowed size {}",
                    size,
                    self.config.content_rules.max_image_size
                )));
            }

            let format = image.get_format()?;
            if !self.config.content_rules.allowed_image_formats.contains(&format) {
                return Err(ValidationError::Format(format!(
                    "Image format {} is not allowed",
                    format
                )));
            }
        }

        // Validate text content
        let text = doc.get_text_content()?;
        if text.len() > self.config.content_rules.max_text_length {
            return Err(ValidationError::Content(format!(
                "Text length {} exceeds maximum allowed length {}",
                text.len(),
                self.config.content_rules.max_text_length
            )));
        }

        // Check for banned keywords
        for keyword in &self.config.content_rules.banned_keywords {
            if text.to_lowercase().contains(&keyword.to_lowercase()) {
                return Err(ValidationError::Content(format!(
                    "Banned keyword {} found in content",
                    keyword
                )));
            }
        }

        // Validate content patterns
        for (name, pattern) in &self.config.content_rules.content_patterns {
            let regex = Regex::new(pattern).map_err(|e| ValidationError::Content(e.to_string()))?;
            if regex.is_match(&text) {
                return Err(ValidationError::Content(format!(
                    "Content matches forbidden pattern {}",
                    name
                )));
            }
        }

        Ok(())
    }

    /// Validates security settings
    #[instrument(skip(self, doc), err(Display))]
    async fn validate_security(&self, doc: &Document) -> ValidationResult<()> {
        let security = doc.get_security()?;

        // Validate encryption
        if let Some(encryption) = security.get_encryption()? {
            if encryption.get_key_length()? < self.config.security_rules.min_key_length {
                return Err(ValidationError::Security(format!(
                    "Encryption key length {} is below minimum required length {}",
                    encryption.get_key_length()?,
                    self.config.security_rules.min_key_length
                )));
            }

            let algorithm = encryption.get_algorithm()?;
            if !self.config.security_rules.required_algorithms.contains(&algorithm) {
                return Err(ValidationError::Security(format!(
                    "Encryption algorithm {} is not allowed",
                    algorithm
                )));
            }
        }

        // Validate permissions
        let permissions = security.get_permissions()?;
        if permissions > self.config.security_rules.max_permission_level {
            return Err(ValidationError::Security(format!(
                "Permission level {} exceeds maximum allowed level {}",
                permissions,
                self.config.security_rules.max_permission_level
            )));
        }

        // Validate security handlers
        let handler = security.get_handler()?;
        if !self.config.security_rules.required_handlers.contains(&handler) {
            return Err(ValidationError::Security(format!(
                "Security handler {} is not allowed",
                handler
            )));
        }

        Ok(())
    }

    /// Validates an email address
    #[instrument(skip(self))]
    pub fn validate_email(&self, email: &str) -> bool {
        EMAIL_REGEX.is_match(email)
    }

    /// Validates a URL
    #[instrument(skip(self))]
    pub fn validate_url(&self, url: &str) -> bool {
        URL_REGEX.is_match(url)
    }

    /// Validates a phone number
    #[instrument(skip(self))]
    pub fn validate_phone(&self, phone: &str) -> bool {
        PHONE_REGEX.is_match(phone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    #[test]
    async fn test_file_size_validation() {
        let config = ValidationConfig {
            max_file_size: 1000,
            ..Default::default()
        };
        let validator = ValidationUtils::new(config);
        let mut doc = Document::new();
        doc.set_content(&vec![0; 2000]);
        
        let result = validator.validate_file_size(&doc).await;
        assert!(result.is_err());
    }

    #[test]
    async fn test_metadata_validation() {
        let validator = ValidationUtils::new(ValidationConfig::default());
        let mut doc = Document::new();
        doc.set_metadata("Creator", "Test");
        doc.set_metadata("Producer", "Test");
        
        let result = validator.validate_metadata(&doc).await;
        assert!(result.is_err());
    }

    #[test]
    async fn test_content_validation() {
        let config = ValidationConfig {
            content_rules: ContentRules {
                banned_keywords: vec!["javascript".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        let validator = ValidationUtils::new(config);
        let mut doc = Document::new();
        doc.set_text_content("This contains javascript code");
        
        let result = validator.validate_content(&doc).await;
        assert!(result.is_err());
    }

    #[test]
    async fn test_security_validation() {
        let config = ValidationConfig {
            security_rules: SecurityRules {
                min_key_length: 256,
                ..Default::default()
            },
            ..Default::default()
        };
        let validator = ValidationUtils::new(config);
        let mut doc = Document::new();
        doc.set_encryption_key_length(128);
        
        let result = validator.validate_security(&doc).await;
        assert!(result.is_err());
    }

    #[test]
    async fn test_regex_validation() {
        let validator = ValidationUtils::new(ValidationConfig::default());
        
        assert!(validator.validate_email("test@example.com"));
        assert!(!validator.validate_email("invalid-email"));
        
        assert!(validator.validate_url("https://example.com"));
        assert!(!validator.validate_url("invalid-url"));
        
        assert!(validator.validate_phone("+1-234-567-8900"));
        assert!(!validator.validate_phone("abc"));
    }
}
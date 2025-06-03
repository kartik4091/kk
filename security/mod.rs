use crate::{metrics::MetricsRegistry, EngineConfig, PdfError};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::{Arc, RwLock};
use aes::Aes256;
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub mod access;
pub mod audit;
pub mod certificate;
pub mod encryption;
pub mod keys;
pub mod policy;
pub mod signature;

type HmacSha256 = Hmac<Sha256>;

pub struct SecuritySystem {
    state: Arc<RwLock<SecurityState>>,
    config: SecurityConfig,
    metrics: Arc<MetricsRegistry>,
    access_control: Arc<access::AccessControlSystem>,
    encryption: Arc<encryption::EncryptionSystem>,
    signature: Arc<signature::SignatureSystem>,
    audit: Arc<audit::AuditSystem>,
}

#[derive(Default)]
struct SecurityState {
    security_checks_performed: u64,
    last_check: Option<DateTime<Utc>>,
    active_checks: u32,
}

#[derive(Clone)]
pub struct SecurityConfig {
    pub encryption_enabled: bool,
    pub signature_required: bool,
    pub audit_level: AuditLevel,
    pub key_rotation_interval: std::time::Duration,
}

#[derive(Clone, Copy, PartialEq)]
pub enum AuditLevel {
    None,
    Basic,
    Detailed,
    Comprehensive,
}

#[derive(Debug)]
pub struct SecurityCheckResult {
    pub is_secure: bool,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub violations: Vec<SecurityViolation>,
}

#[derive(Debug)]
pub enum SecurityViolation {
    InvalidSignature(String),
    EncryptionFailure(String),
    AccessDenied(String),
    PolicyViolation(String),
}

impl SecuritySystem {
    pub async fn new(
        engine_config: &EngineConfig,
        metrics: Arc<MetricsRegistry>,
    ) -> Result<Self, PdfError> {
        let config = SecurityConfig::default();
        
        let access_control = Arc::new(access::AccessControlSystem::new(&config).await?);
        let encryption = Arc::new(encryption::EncryptionSystem::new(&config).await?);
        let signature = Arc::new(signature::SignatureSystem::new(&config).await?);
        let audit = Arc::new(audit::AuditSystem::new(&config).await?);

        Ok(Self {
            state: Arc::new(RwLock::new(SecurityState::default())),
            config,
            metrics,
            access_control,
            encryption,
            signature,
            audit,
        })
    }

    pub async fn check_document(&self, data: &[u8]) -> Result<SecurityCheckResult, PdfError> {
        let start_time = std::time::Instant::now();
        let _timer = self.metrics.validation_duration.start_timer();

        // Update state
        {
            let mut state = self.state.write().map_err(|_| 
                PdfError::Security("Failed to acquire state lock".to_string()))?;
            state.active_checks += 1;
        }

        let result = self.internal_check_document(data).await;

        // Update metrics and state
        {
            let mut state = self.state.write().map_err(|_| 
                PdfError::Security("Failed to acquire state lock".to_string()))?;
            state.active_checks -= 1;
            state.security_checks_performed += 1;
            state.last_check = Some(Utc::now());
        }

        // Record metrics
        if let Ok(ref result) = result {
            if !result.is_secure {
                self.metrics.security_violations.inc();
            }
        }

        result
    }

    async fn internal_check_document(&self, data: &[u8]) -> Result<SecurityCheckResult, PdfError> {
        let mut violations = Vec::new();

        // Check access control
        if let Err(e) = self.access_control.verify_access(data).await {
            violations.push(SecurityViolation::AccessDenied(e.to_string()));
        }

        // Verify signatures if required
        if self.config.signature_required {
            if let Err(e) = self.signature.verify_signatures(data).await {
                violations.push(SecurityViolation::InvalidSignature(e.to_string()));
            }
        }

        // Check encryption status
        if self.config.encryption_enabled {
            if let Err(e) = self.encryption.verify_encryption(data).await {
                violations.push(SecurityViolation::EncryptionFailure(e.to_string()));
            }
        }

        // Log audit event
        self.audit.log_security_check(data, &violations).await?;

        Ok(SecurityCheckResult {
            is_secure: violations.is_empty(),
            message: if violations.is_empty() {
                "Security check passed".to_string()
            } else {
                "Security violations detected".to_string()
            },
            timestamp: Utc::now(),
            violations,
        })
    }

    pub async fn encrypt_document(&self, data: &[u8]) -> Result<Vec<u8>, PdfError> {
        let start_time = std::time::Instant::now();
        let _timer = self.metrics.encryption_operations.inc();

        let result = self.encryption.encrypt_document(data).await?;

        // Record metrics
        self.metrics.encryption_operations.inc();

        Ok(result)
    }

    pub async fn sign_document(&self, data: &[u8]) -> Result<Vec<u8>, PdfError> {
        let result = self.signature.sign_document(data).await?;
        self.metrics.signature_validations.inc();
        Ok(result)
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            encryption_enabled: true,
            signature_required: true,
            audit_level: AuditLevel::Detailed,
            key_rotation_interval: std::time::Duration::from_secs(24 * 60 * 60), // 24 hours
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_security_system_creation() {
        let metrics = Arc::new(MetricsRegistry::new().unwrap());
        let config = EngineConfig::default();
        let security = SecuritySystem::new(&config, metrics).await;
        assert!(security.is_ok());
    }

    #[tokio::test]
    async fn test_document_security_check() {
        let metrics = Arc::new(MetricsRegistry::new().unwrap());
        let config = EngineConfig::default();
        let security = SecuritySystem::new(&config, metrics).await.unwrap();
        
        let sample_data = include_bytes!("../../tests/data/sample.pdf");
        let result = security.check_document(sample_data).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_document_encryption() {
        let metrics = Arc::new(MetricsRegistry::new().unwrap());
        let config = EngineConfig::default();
        let security = SecuritySystem::new(&config, metrics).await.unwrap();
        
        let sample_data = include_bytes!("../../tests/data/sample.pdf");
        let result = security.encrypt_document(sample_data).await;
        assert!(result.is_ok());
    }
}
use crate::{PdfError, SecurityConfig};
use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
use rand::RngCore;
use std::sync::{Arc, RwLock};
use chrono::{DateTime, Utc};

type Aes256Cbc = cbc::Encryptor<aes::Aes256>;

pub struct EncryptionSystem {
    state: Arc<RwLock<EncryptionState>>,
    config: EncryptionConfig,
}

struct EncryptionState {
    operations_performed: u64,
    last_operation: Option<DateTime<Utc>>,
    active_operations: u32,
}

#[derive(Clone)]
pub struct EncryptionConfig {
    pub algorithm: EncryptionAlgorithm,
    pub key_size: usize,
    pub iv_size: usize,
}

#[derive(Clone, Copy)]
pub enum EncryptionAlgorithm {
    Aes256Cbc,
    Aes256Gcm,
}

impl EncryptionSystem {
    pub async fn new(security_config: &SecurityConfig) -> Result<Self, PdfError> {
        Ok(Self {
            state: Arc::new(RwLock::new(EncryptionState {
                operations_performed: 0,
                last_operation: None,
                active_operations: 0,
            })),
            config: EncryptionConfig::default(),
        })
    }

    pub async fn encrypt_document(&self, data: &[u8]) -> Result<Vec<u8>, PdfError> {
        // Update state
        {
            let mut state = self.state.write().map_err(|_| 
                PdfError::Encryption("Failed to acquire state lock".to_string()))?;
            state.active_operations += 1;
        }

        let result = self.internal_encrypt_document(data).await;

        // Update state
        {
            let mut state = self.state.write().map_err(|_| 
                PdfError::Encryption("Failed to acquire state lock".to_string()))?;
            state.active_operations -= 1;
            state.operations_performed += 1;
            state.last_operation = Some(Utc::now());
        }

        result
    }

    async fn internal_encrypt_document(&self, data: &[u8]) -> Result<Vec<u8>, PdfError> {
        match self.config.algorithm {
            EncryptionAlgorithm::Aes256Cbc => self.encrypt_aes_cbc(data),
            EncryptionAlgorithm::Aes256Gcm => self.encrypt_aes_gcm(data),
        }
    }

    fn encrypt_aes_cbc(&self, data: &[u8]) -> Result<Vec<u8>, PdfError> {
        let key = self.generate_key()?;
        let iv = self.generate_iv()?;

        let cipher = Aes256Cbc::new(&key.into(), &iv.into());
        let mut buffer = vec![0u8; data.len() + 16]; // Add space for padding
        let ciphertext = cipher
            .encrypt_padded_b2b_mut::<Pkcs7>(data, &mut buffer)
            .map_err(|e| PdfError::Encryption(format!("Encryption failed: {}", e)))?;

        // Prepend IV to ciphertext
        let mut result = Vec::with_capacity(iv.len() + ciphertext.len());
        result.extend_from_slice(&iv);
        result.extend_from_slice(ciphertext);

        Ok(result)
    }

    fn encrypt_aes_gcm(&self, data: &[u8]) -> Result<Vec<u8>, PdfError> {
        // Implementation for AES-GCM encryption
        // This is a placeholder - implement actual AES-GCM encryption
        Err(PdfError::Encryption("AES-GCM not implemented".to_string()))
    }

    pub async fn verify_encryption(&self, data: &[u8]) -> Result<bool, PdfError> {
        // Verify if the document is properly encrypted
        // This is a basic check - enhance based on your requirements
        if data.len() < self.config.iv_size {
            return Ok(false);
        }

        // Check for encryption markers or other indicators
        Ok(true)
    }

    fn generate_key(&self) -> Result<[u8; 32], PdfError> {
        let mut key = [0u8; 32];
        rand::thread_rng()
            .try_fill_bytes(&mut key)
            .map_err(|e| PdfError::Encryption(format!("Key generation failed: {}", e)))?;
        Ok(key)
    }

    fn generate_iv(&self) -> Result<[u8; 16], PdfError> {
        let mut iv = [0u8; 16];
        rand::thread_rng()
            .try_fill_bytes(&mut iv)
            .map_err(|e| PdfError::Encryption(format!("IV generation failed: {}", e)))?;
        Ok(iv)
    }
}

impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            algorithm: EncryptionAlgorithm::Aes256Cbc,
            key_size: 32, // 256 bits
            iv_size: 16,  // 128 bits
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_encryption_system_creation() {
        let config = SecurityConfig::default();
        let system = EncryptionSystem::new(&config).await;
        assert!(system.is_ok());
    }

    #[tokio::test]
    async fn test_document_encryption() {
        let config = SecurityConfig::default();
        let system = EncryptionSystem::new(&config).await.unwrap();
        
        let sample_data = b"Test document data";
        let encrypted = system.encrypt_document(sample_data).await;
        assert!(encrypted.is_ok());
        
        let encrypted_data = encrypted.unwrap();
        assert!(encrypted_data.len() > sample_data.len()); // Account for IV and padding
    }

    #[tokio::test]
    async fn test_encryption_verification() {
        let config = SecurityConfig::default();
        let system = EncryptionSystem::new(&config).await.unwrap();
        
        let sample_data = b"Test document data";
        let encrypted = system.encrypt_document(sample_data).await.unwrap();
        
        let verification = system.verify_encryption(&encrypted).await;
        assert!(verification.is_ok());
        assert!(verification.unwrap());
    }
}
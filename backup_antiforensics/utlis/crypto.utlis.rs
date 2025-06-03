//! Cryptographic utilities for PDF antiforensics
//! Author: kartik4091
//! Created: 2025-06-03 04:47:15 UTC
//! This module provides cryptographic functions for secure 
//! data handling and transformation.

use std::{
    io::{self, Read, Write},
    path::Path,
    time::{Duration, Instant},
};
use ring::{
    aead,
    digest::{Context, SHA256},
    pbkdf2,
    rand::{SecureRandom, SystemRandom},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Serialize, Deserialize};
use thiserror::Error;
use tracing::{info, warn, error, debug, trace, instrument};

/// Cryptographic error types
#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("Key derivation failed: {0}")]
    KeyDerivation(String),
    
    #[error("Encryption failed: {0}")]
    Encryption(String),
    
    #[error("Decryption failed: {0}")]
    Decryption(String),
    
    #[error("Hash operation failed: {0}")]
    Hash(String),
    
    #[error("Random number generation failed: {0}")]
    Random(String),
    
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
}

/// Result type for cryptographic operations
pub type CryptoResult<T> = Result<T, CryptoError>;

/// Encryption configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionConfig {
    /// Key derivation iterations
    pub pbkdf2_iterations: u32,
    /// Salt length in bytes
    pub salt_length: usize,
    /// Nonce length in bytes
    pub nonce_length: usize,
    /// Tag length in bytes
    pub tag_length: usize,
}

impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            pbkdf2_iterations: 100_000,
            salt_length: 16,
            nonce_length: 12,
            tag_length: 16,
        }
    }
}

/// Cryptographic utilities implementation
pub struct CryptoUtils {
    /// Random number generator
    rng: SystemRandom,
    /// Encryption configuration
    config: EncryptionConfig,
}

impl CryptoUtils {
    /// Creates a new cryptographic utilities instance
    #[instrument(skip(config))]
    pub fn new(config: EncryptionConfig) -> Self {
        debug!("Initializing CryptoUtils");
        
        Self {
            rng: SystemRandom::new(),
            config,
        }
    }

    /// Encrypts data using AES-GCM
    #[instrument(skip(self, data, key), err(Display))]
    pub fn encrypt(&self, data: &[u8], key: &[u8]) -> CryptoResult<Vec<u8>> {
        // Generate salt for key derivation
        let mut salt = vec![0u8; self.config.salt_length];
        self.rng
            .fill(&mut salt)
            .map_err(|e| CryptoError::Random(e.to_string()))?;

        // Derive encryption key
        let encryption_key = self.derive_key(key, &salt)?;

        // Generate nonce
        let mut nonce = vec![0u8; self.config.nonce_length];
        self.rng
            .fill(&mut nonce)
            .map_err(|e| CryptoError::Random(e.to_string()))?;

        // Create sealing key
        let sealing_key = aead::UnboundKey::new(&aead::AES_256_GCM, &encryption_key)
            .map_err(|e| CryptoError::Encryption(e.to_string()))?;
        let sealing_key = aead::LessSafeKey::new(sealing_key);

        // Encrypt data
        let mut in_out = data.to_vec();
        sealing_key
            .seal_in_place_append_tag(
                aead::Nonce::try_assume_unique_for_key(&nonce)
                    .map_err(|e| CryptoError::Encryption(e.to_string()))?,
                aead::Aad::empty(),
                &mut in_out,
            )
            .map_err(|e| CryptoError::Encryption(e.to_string()))?;

        // Combine salt, nonce, and encrypted data
        let mut result = Vec::with_capacity(
            salt.len() + nonce.len() + in_out.len()
        );
        result.extend_from_slice(&salt);
        result.extend_from_slice(&nonce);
        result.extend_from_slice(&in_out);

        Ok(result)
    }

    /// Decrypts data using AES-GCM
    #[instrument(skip(self, encrypted_data, key), err(Display))]
    pub fn decrypt(&self, encrypted_data: &[u8], key: &[u8]) -> CryptoResult<Vec<u8>> {
        // Extract salt, nonce, and ciphertext
        if encrypted_data.len() < self.config.salt_length + self.config.nonce_length + self.config.tag_length {
            return Err(CryptoError::InvalidInput("Data too short".into()));
        }

        let (salt, rest) = encrypted_data.split_at(self.config.salt_length);
        let (nonce, ciphertext) = rest.split_at(self.config.nonce_length);

        // Derive decryption key
        let decryption_key = self.derive_key(key, salt)?;

        // Create opening key
        let opening_key = aead::UnboundKey::new(&aead::AES_256_GCM, &decryption_key)
            .map_err(|e| CryptoError::Decryption(e.to_string()))?;
        let opening_key = aead::LessSafeKey::new(opening_key);

        // Decrypt data
        let mut in_out = ciphertext.to_vec();
        let decrypted_data = opening_key
            .open_in_place(
                aead::Nonce::try_assume_unique_for_key(nonce)
                    .map_err(|e| CryptoError::Decryption(e.to_string()))?,
                aead::Aad::empty(),
                &mut in_out,
            )
            .map_err(|e| CryptoError::Decryption(e.to_string()))?;

        Ok(decrypted_data.to_vec())
    }

    /// Derives an encryption key using PBKDF2
    #[instrument(skip(self, password, salt), err(Display))]
    fn derive_key(&self, password: &[u8], salt: &[u8]) -> CryptoResult<Vec<u8>> {
        let mut key = vec![0u8; 32]; // 256-bit key
        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA256,
            self.config.pbkdf2_iterations.try_into().unwrap(),
            salt,
            password,
            &mut key,
        );
        Ok(key)
    }

    /// Generates a secure random key
    #[instrument(skip(self), err(Display))]
    pub fn generate_key(&self, length: usize) -> CryptoResult<Vec<u8>> {
        let mut key = vec![0u8; length];
        self.rng
            .fill(&mut key)
            .map_err(|e| CryptoError::Random(e.to_string()))?;
        Ok(key)
    }

    /// Calculates SHA-256 hash of data
    #[instrument(skip(self, data))]
    pub fn hash_sha256(&self, data: &[u8]) -> String {
        let mut context = Context::new(&SHA256);
        context.update(data);
        let digest = context.finish();
        hex::encode(digest.as_ref())
    }

    /// Encrypts data to a file
    #[instrument(skip(self, data, key), err(Display))]
    pub async fn encrypt_to_file<P: AsRef<Path>>(
        &self,
        data: &[u8],
        key: &[u8],
        path: P,
    ) -> CryptoResult<()> {
        let encrypted = self.encrypt(data, key)?;
        tokio::fs::write(path, encrypted).await?;
        Ok(())
    }

    /// Decrypts data from a file
    #[instrument(skip(self, key), err(Display))]
    pub async fn decrypt_from_file<P: AsRef<Path>>(
        &self,
        key: &[u8],
        path: P,
    ) -> CryptoResult<Vec<u8>> {
        let encrypted = tokio::fs::read(path).await?;
        self.decrypt(&encrypted, key)
    }

    /// Generates a secure random nonce
    #[instrument(skip(self), err(Display))]
    pub fn generate_nonce(&self) -> CryptoResult<Vec<u8>> {
        let mut nonce = vec![0u8; self.config.nonce_length];
        self.rng
            .fill(&mut nonce)
            .map_err(|e| CryptoError::Random(e.to_string()))?;
        Ok(nonce)
    }

    /// Encodes data as base64
    #[instrument(skip(self, data))]
    pub fn encode_base64(&self, data: &[u8]) -> String {
        BASE64.encode(data)
    }

    /// Decodes base64 data
    #[instrument(skip(self, data), err(Display))]
    pub fn decode_base64(&self, data: &str) -> CryptoResult<Vec<u8>> {
        BASE64.decode(data)
            .map_err(|e| CryptoError::InvalidInput(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    #[test]
    async fn test_encrypt_decrypt() {
        let crypto = CryptoUtils::new(EncryptionConfig::default());
        let data = b"Hello, World!";
        let key = crypto.generate_key(32).unwrap();
        
        let encrypted = crypto.encrypt(data, &key).unwrap();
        let decrypted = crypto.decrypt(&encrypted, &key).unwrap();
        
        assert_eq!(data, &decrypted[..]);
    }

    #[test]
    async fn test_key_derivation() {
        let crypto = CryptoUtils::new(EncryptionConfig::default());
        let password = b"password123";
        let salt = crypto.generate_key(16).unwrap();
        
        let key1 = crypto.derive_key(password, &salt).unwrap();
        let key2 = crypto.derive_key(password, &salt).unwrap();
        
        assert_eq!(key1, key2);
    }

    #[test]
    async fn test_file_encryption() {
        let crypto = CryptoUtils::new(EncryptionConfig::default());
        let data = b"Secret data";
        let key = crypto.generate_key(32).unwrap();
        let path = "test_encrypted.dat";
        
        crypto.encrypt_to_file(data, &key, path).await.unwrap();
        let decrypted = crypto.decrypt_from_file(&key, path).await.unwrap();
        
        assert_eq!(data, &decrypted[..]);
        tokio::fs::remove_file(path).await.unwrap();
    }

    #[test]
    async fn test_hash_sha256() {
        let crypto = CryptoUtils::new(EncryptionConfig::default());
        let data = b"Test data";
        let hash1 = crypto.hash_sha256(data);
        let hash2 = crypto.hash_sha256(data);
        
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // 32 bytes in hex
    }

    #[test]
    async fn test_base64() {
        let crypto = CryptoUtils::new(EncryptionConfig::default());
        let data = b"Test data";
        
        let encoded = crypto.encode_base64(data);
        let decoded = crypto.decode_base64(&encoded).unwrap();
        
        assert_eq!(data, &decoded[..]);
    }
}
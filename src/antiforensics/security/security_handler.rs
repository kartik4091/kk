//! Security handler implementation for PDF anti-forensics
//! Created: 2025-06-03 15:32:23 UTC
//! Author: kartik4091

use std::collections::HashMap;
use aes::{Aes256, Aes128, cipher::{BlockEncrypt, BlockDecrypt}};
use rc4::{KeyInit, StreamCipher, Rc4};
use sha2::{Sha256, Digest};
use tracing::{debug, error, info, instrument, warn};

use crate::{
    error::{Error, Result},
    types::{Document, Object, ObjectId},
};

/// Handles PDF security and encryption operations
#[derive(Debug)]
pub struct SecurityHandler {
    /// Security statistics
    stats: SecurityStats,
    
    /// Current encryption key
    current_key: Option<Vec<u8>>,
    
    /// Object keys cache
    object_keys: HashMap<ObjectId, Vec<u8>>,
    
    /// Permission cache
    permission_cache: HashMap<ObjectId, u32>,
}

/// Security processing statistics
#[derive(Debug, Default)]
pub struct SecurityStats {
    /// Number of objects encrypted
    pub objects_encrypted: usize,
    
    /// Number of objects decrypted
    pub objects_decrypted: usize,
    
    /// Number of permissions updated
    pub permissions_updated: usize,
    
    /// Number of keys generated
    pub keys_generated: usize,
    
    /// Processing duration in milliseconds
    pub duration_ms: u64,
}

/// Security configuration
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Encryption method
    pub encryption_method: EncryptionMethod,
    
    /// Key length in bits
    pub key_length: u32,
    
    /// Use AES encryption
    pub use_aes: bool,
    
    /// Metadata encryption
    pub encrypt_metadata: bool,
    
    /// Permission flags
    pub permissions: u32,
    
    /// Owner password
    pub owner_password: Option<String>,
    
    /// User password
    pub user_password: Option<String>,
}

/// Supported encryption methods
#[derive(Debug, Clone, PartialEq)]
pub enum EncryptionMethod {
    /// RC4 encryption
    RC4,
    
    /// AES encryption
    AES,
    
    /// Identity (no encryption)
    Identity,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            encryption_method: EncryptionMethod::AES,
            key_length: 256,
            use_aes: true,
            encrypt_metadata: true,
            permissions: 0xFFFFF0C0, // All permissions allowed
            owner_password: None,
            user_password: None,
        }
    }
}

impl SecurityHandler {
    /// Create new security handler instance
    pub fn new() -> Result<Self> {
        Ok(Self {
            stats: SecurityStats::default(),
            current_key: None,
            object_keys: HashMap::new(),
            permission_cache: HashMap::new(),
        })
    }
    
    /// Configure security settings
    #[instrument(skip(self, config))]
    pub fn configure(&mut self, config: &SecurityConfig) -> Result<()> {
        // Validate configuration
        self.validate_config(config)?;
        
        // Generate encryption key if needed
        if config.encryption_method != EncryptionMethod::Identity {
            self.generate_encryption_key(config)?;
        }
        
        debug!("Security handler configured successfully");
        Ok(())
    }
    
    /// Process document security
    #[instrument(skip(self, document, config))]
    pub fn process_security(&mut self, document: &mut Document, config: &SecurityConfig) -> Result<()> {
        let start_time = std::time::Instant::now();
        info!("Starting security processing");
        
        // Update encryption dictionary
        self.update_encryption_dictionary(document, config)?;
        
        // Process objects based on encryption method
        match config.encryption_method {
            EncryptionMethod::RC4 => {
                self.process_rc4_encryption(document, config)?;
            }
            EncryptionMethod::AES => {
                self.process_aes_encryption(document, config)?;
            }
            EncryptionMethod::Identity => {
                debug!("No encryption required");
            }
        }
        
        self.stats.duration_ms = start_time.elapsed().as_millis() as u64;
        info!("Security processing completed");
        Ok(())
    }
    
    /// Validate security configuration
    fn validate_config(&self, config: &SecurityConfig) -> Result<()> {
        // Validate key length
        match config.key_length {
            40 | 128 | 256 => {}
            _ => return Err(Error::InvalidKeyLength),
        }
        
        // Validate encryption method and key length combination
        match (config.encryption_method, config.key_length) {
            (EncryptionMethod::RC4, 40) => {}
            (EncryptionMethod::RC4, 128) => {}
            (EncryptionMethod::AES, 128) => {}
            (EncryptionMethod::AES, 256) => {}
            (EncryptionMethod::Identity, _) => {}
            _ => return Err(Error::InvalidEncryptionConfig),
        }
        
        Ok(())
    }
    
    /// Generate encryption key
    fn generate_encryption_key(&mut self, config: &SecurityConfig) -> Result<()> {
        let key = match config.encryption_method {
            EncryptionMethod::RC4 => {
                self.generate_rc4_key(config)?
            }
            EncryptionMethod::AES => {
                self.generate_aes_key(config)?
            }
            EncryptionMethod::Identity => {
                return Ok(());
            }
        };
        
        self.current_key = Some(key);
        self.stats.keys_generated += 1;
        Ok(())
    }
    
    /// Generate RC4 encryption key
    fn generate_rc4_key(&self, config: &SecurityConfig) -> Result<Vec<u8>> {
        let mut key = Vec::new();
        
        // Implementation based on PDF spec
        let mut hasher = Sha256::new();
        
        // Add user password if provided
        if let Some(pass) = &config.user_password {
            hasher.update(pass.as_bytes());
        }
        
        // Add owner password if provided
        if let Some(pass) = &config.owner_password {
            hasher.update(pass.as_bytes());
        }
        
        // Add permissions
        hasher.update(&config.permissions.to_le_bytes());
        
        // Generate key
        key = hasher.finalize()[..config.key_length as usize / 8].to_vec();
        
        Ok(key)
    }
    
    /// Generate AES encryption key
    fn generate_aes_key(&self, config: &SecurityConfig) -> Result<Vec<u8>> {
        let mut key = Vec::new();
        
        // Implementation based on PDF spec
        let mut hasher = Sha256::new();
        
        // Add user password if provided
        if let Some(pass) = &config.user_password {
            hasher.update(pass.as_bytes());
        }
        
        // Add owner password if provided
        if let Some(pass) = &config.owner_password {
            hasher.update(pass.as_bytes());
        }
        
        // Add permissions
        hasher.update(&config.permissions.to_le_bytes());
        
        // Generate key
        key = hasher.finalize()[..config.key_length as usize / 8].to_vec();
        
        Ok(key)
    }
    
    /// Update encryption dictionary
    fn update_encryption_dictionary(&mut self, document: &mut Document, config: &SecurityConfig) -> Result<()> {
        let mut dict = HashMap::new();
        
        // Set filter
        dict.insert(b"Filter".to_vec(), Object::Name(b"Standard".to_vec()));
        
        // Set version and revision based on configuration
        let (version, revision) = match (config.encryption_method, config.key_length) {
            (EncryptionMethod::RC4, 40) => (1, 2),
            (EncryptionMethod::RC4, 128) => (2, 3),
            (EncryptionMethod::AES, 128) => (4, 4),
            (EncryptionMethod::AES, 256) => (5, 5),
            _ => return Err(Error::InvalidEncryptionConfig),
        };
        
        dict.insert(b"V".to_vec(), Object::Integer(version));
        dict.insert(b"R".to_vec(), Object::Integer(revision));
        
        // Set permissions
        dict.insert(b"P".to_vec(), Object::Integer(config.permissions as i32));
        
        // Set encryption method specific parameters
        match config.encryption_method {
            EncryptionMethod::RC4 => {
                dict.insert(b"CF".to_vec(), self.create_rc4_crypt_filter(config)?);
            }
            EncryptionMethod::AES => {
                dict.insert(b"CF".to_vec(), self.create_aes_crypt_filter(config)?);
            }
            EncryptionMethod::Identity => {}
        }
        
        // Update document encryption dictionary
        document.structure.trailer.encrypt = Some(Object::Dictionary(dict));
        
        Ok(())
    }
    
    /// Create RC4 crypt filter
    fn create_rc4_crypt_filter(&self, config: &SecurityConfig) -> Result<Object> {
        let mut cf_dict = HashMap::new();
        let mut std_cf = HashMap::new();
        
        std_cf.insert(b"AuthEvent".to_vec(), Object::Name(b"DocOpen".to_vec()));
        std_cf.insert(b"CFM".to_vec(), Object::Name(b"V2".to_vec()));
        std_cf.insert(b"Length".to_vec(), Object::Integer(config.key_length as i32));
        
        cf_dict.insert(b"StdCF".to_vec(), Object::Dictionary(std_cf));
        
        Ok(Object::Dictionary(cf_dict))
    }
    
    /// Create AES crypt filter
    fn create_aes_crypt_filter(&self, config: &SecurityConfig) -> Result<Object> {
        let mut cf_dict = HashMap::new();
        let mut std_cf = HashMap::new();
        
        std_cf.insert(b"AuthEvent".to_vec(), Object::Name(b"DocOpen".to_vec()));
        std_cf.insert(b"CFM".to_vec(), Object::Name(b"AESV3".to_vec()));
        std_cf.insert(b"Length".to_vec(), Object::Integer(config.key_length as i32));
        
        cf_dict.insert(b"StdCF".to_vec(), Object::Dictionary(std_cf));
        
        Ok(Object::Dictionary(cf_dict))
    }
    
    /// Process RC4 encryption
    fn process_rc4_encryption(&mut self, document: &mut Document, config: &SecurityConfig) -> Result<()> {
        for (id, object) in &mut document.structure.objects {
            if self.should_encrypt_object(id, object) {
                self.encrypt_object_rc4(id, object)?;
                self.stats.objects_encrypted += 1;
            }
        }
        Ok(())
    }
    
    /// Process AES encryption
    fn process_aes_encryption(&mut self, document: &mut Document, config: &SecurityConfig) -> Result<()> {
        for (id, object) in &mut document.structure.objects {
            if self.should_encrypt_object(id, object) {
                self.encrypt_object_aes(id, object)?;
                self.stats.objects_encrypted += 1;
            }
        }
        Ok(())
    }
    
    /// Determine if object should be encrypted
    fn should_encrypt_object(&self, id: &ObjectId, object: &Object) -> bool {
        // Don't encrypt encryption dictionary or document catalog
        if id.number == 1 || matches!(object, Object::Dictionary(dict) if dict.contains_key(b"Type") && dict.get(b"Type") == Some(&Object::Name(b"Catalog".to_vec()))) {
            return false;
        }
        true
    }
    
    /// Encrypt object using RC4
    fn encrypt_object_rc4(&mut self, id: &ObjectId, object: &mut Object) -> Result<()> {
        if let Some(key) = &self.current_key {
            let obj_key = self.generate_object_key(id, key)?;
            
            match object {
                Object::String(data) | Object::Stream { data, .. } => {
                    let mut cipher = Rc4::new(&obj_key);
                    cipher.apply_keystream(data);
                }
                _ => {}
            }
        }
        Ok(())
    }
    
    /// Encrypt object using AES
    fn encrypt_object_aes(&mut self, id: &ObjectId, object: &mut Object) -> Result<()> {
        if let Some(key) = &self.current_key {
            let obj_key = self.generate_object_key(id, key)?;
            
            match object {
                Object::String(data) | Object::Stream { data, .. } => {
                    // Implementation depends on key length
                    if key.len() == 32 {
                        let cipher = Aes256::new(&obj_key[..32].into());
                        // Process data in blocks
                        for chunk in data.chunks_mut(16) {
                            let mut block = [0u8; 16];
                            block[..chunk.len()].copy_from_slice(chunk);
                            cipher.encrypt_block((&mut block).into());
                            chunk.copy_from_slice(&block[..chunk.len()]);
                        }
                    } else {
                        let cipher = Aes128::new(&obj_key[..16].into());
                        // Process data in blocks
                        for chunk in data.chunks_mut(16) {
                            let mut block = [0u8; 16];
                            block[..chunk.len()].copy_from_slice(chunk);
                            cipher.encrypt_block((&mut block).into());
                            chunk.copy_from_slice(&block[..chunk.len()]);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    
    /// Generate object-specific encryption key
    fn generate_object_key(&mut self, id: &ObjectId, base_key: &[u8]) -> Result<Vec<u8>> {
        if let Some(key) = self.object_keys.get(id) {
            return Ok(key.clone());
        }
        
        let mut hasher = Sha256::new();
        hasher.update(base_key);
        hasher.update(&id.number.to_le_bytes());
        hasher.update(&id.generation.to_le_bytes());
        
        let key = hasher.finalize()[..base_key.len()].to_vec();
        self.object_keys.insert(*id, key.clone());
        
        Ok(key)
    }
    
    /// Get security statistics
    pub fn statistics(&self) -> &SecurityStats {
        &self.stats
    }
    
    /// Reset handler state
    pub fn reset(&mut self) {
        self.stats = SecurityStats::default();
        self.current_key = None;
        self.object_keys.clear();
        self.permission_cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn setup_test_handler() -> SecurityHandler {
        SecurityHandler::new().unwrap()
    }
    
    #[test]
    fn test_handler_initialization() {
        let handler = setup_test_handler();
        assert!(handler.current_key.is_none());
        assert!(handler.object_keys.is_empty());
    }
    
    #[test]
    fn test_configuration_validation() {
        let handler = setup_test_handler();
        
        let valid_config = SecurityConfig {
            encryption_method: EncryptionMethod::AES,
            key_length: 256,
            ..Default::default()
        };
        
        let invalid_config = SecurityConfig {
            encryption_method: EncryptionMethod::AES,
            key_length: 64, // Invalid key length
            ..Default::default()
        };
        
        assert!(handler.validate_config(&valid_config).is_ok());
        assert!(handler.validate_config(&invalid_config).is_err());
    }
    
    #[test]
    fn test_key_generation() {
        let mut handler = setup_test_handler();
        
        let config = SecurityConfig {
            encryption_method: EncryptionMethod::AES,
            key_length: 256,
            user_password: Some("test".to_string()),
            ..Default::default()
        };
        
        assert!(handler.generate_encryption_key(&config).is_ok());
        assert!(handler.current_key.is_some());
    }
    
    #[test]
    fn test_object_key_generation() {
        let mut handler = setup_test_handler();
        let base_key = vec![0u8; 32];
        let id = ObjectId { number: 1, generation: 0 };
        
        let key = handler.generate_object_key(&id, &base_key).unwrap();
        assert_eq!(key.len(), base_key.len());
    }
    
    #[test]
    fn test_handler_reset() {
        let mut handler = setup_test_handler();
        
        // Add some test data
        handler.current_key = Some(vec![0u8; 32]);
        handler.object_keys.insert(ObjectId { number: 1, generation: 0 }, vec![0u8; 32]);
        handler.stats.objects_encrypted = 1;
        
        handler.reset();
        
        assert!(handler.current_key.is_none());
        assert!(handler.object_keys.is_empty());
        assert_eq!(handler.stats.objects_encrypted, 0);
    }
}

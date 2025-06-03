//! Encryption implementation for PDF anti-forensics
//! Created: 2025-06-03 15:34:09 UTC
//! Author: kartik4091

use std::collections::HashMap;
use aes::{Aes256, Aes128, cipher::{BlockEncrypt, BlockDecrypt, KeyInit}};
use rc4::{Rc4, StreamCipher};
use sha2::{Sha256, Digest};
use tracing::{debug, error, info, instrument, warn};

use crate::{
    error::{Error, Result},
    types::{Document, Object, ObjectId, Stream},
};

/// Handles PDF encryption operations
#[derive(Debug)]
pub struct Encryptor {
    /// Encryption statistics
    stats: EncryptionStats,
    
    /// Encryption keys
    keys: KeyStore,
    
    /// Object encryption state
    object_state: HashMap<ObjectId, EncryptionState>,
}

/// Encryption statistics
#[derive(Debug, Default)]
pub struct EncryptionStats {
    /// Number of objects encrypted
    pub objects_encrypted: usize,
    
    /// Number of objects decrypted
    pub objects_decrypted: usize,
    
    /// Number of keys generated
    pub keys_generated: usize,
    
    /// Total bytes processed
    pub bytes_processed: u64,
    
    /// Processing duration in milliseconds
    pub duration_ms: u64,
}

/// Key storage
#[derive(Debug)]
struct KeyStore {
    /// File encryption key
    file_key: Option<Vec<u8>>,
    
    /// Object encryption keys
    object_keys: HashMap<ObjectId, Vec<u8>>,
    
    /// User key
    user_key: Option<Vec<u8>>,
    
    /// Owner key
    owner_key: Option<Vec<u8>>,
}

/// Object encryption state
#[derive(Debug, Clone, PartialEq)]
pub enum EncryptionState {
    /// Object is encrypted
    Encrypted,
    
    /// Object is decrypted
    Decrypted,
    
    /// Object is exempt from encryption
    Exempt,
}

/// Encryption configuration
#[derive(Debug, Clone)]
pub struct EncryptionConfig {
    /// Encryption method
    pub method: EncryptionMethod,
    
    /// Key length in bits
    pub key_length: u32,
    
    /// Encryption key
    pub key: Option<Vec<u8>>,
    
    /// Initialization vector
    pub iv: Option<Vec<u8>>,
    
    /// Encrypt metadata
    pub encrypt_metadata: bool,
    
    /// Encryption revision
    pub revision: u32,
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

impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            method: EncryptionMethod::AES,
            key_length: 256,
            key: None,
            iv: None,
            encrypt_metadata: true,
            revision: 6,
        }
    }
}

impl Encryptor {
    /// Create new encryptor instance
    pub fn new() -> Result<Self> {
        Ok(Self {
            stats: EncryptionStats::default(),
            keys: KeyStore {
                file_key: None,
                object_keys: HashMap::new(),
                user_key: None,
                owner_key: None,
            },
            object_state: HashMap::new(),
        })
    }
    
    /// Configure encryption
    #[instrument(skip(self, config))]
    pub fn configure(&mut self, config: &EncryptionConfig) -> Result<()> {
        // Validate configuration
        self.validate_config(config)?;
        
        // Initialize encryption keys
        self.init_keys(config)?;
        
        debug!("Encryption configured successfully");
        Ok(())
    }
    
    /// Encrypt document
    #[instrument(skip(self, document, config))]
    pub fn encrypt_document(&mut self, document: &mut Document, config: &EncryptionConfig) -> Result<()> {
        let start_time = std::time::Instant::now();
        info!("Starting document encryption");
        
        // Update encryption dictionary
        self.update_encryption_dictionary(document, config)?;
        
        // Encrypt objects
        for (id, object) in &mut document.structure.objects {
            if self.should_encrypt_object(id, object) {
                self.encrypt_object(id, object, config)?;
                self.stats.objects_encrypted += 1;
            }
        }
        
        self.stats.duration_ms = start_time.elapsed().as_millis() as u64;
        info!("Document encryption completed");
        Ok(())
    }
    
    /// Decrypt document
    #[instrument(skip(self, document, config))]
    pub fn decrypt_document(&mut self, document: &mut Document, config: &EncryptionConfig) -> Result<()> {
        let start_time = std::time::Instant::now();
        info!("Starting document decryption");
        
        // Decrypt objects
        for (id, object) in &mut document.structure.objects {
            if self.is_object_encrypted(id) {
                self.decrypt_object(id, object, config)?;
                self.stats.objects_decrypted += 1;
            }
        }
        
        // Clear encryption dictionary
        document.structure.trailer.encrypt = None;
        
        self.stats.duration_ms = start_time.elapsed().as_millis() as u64;
        info!("Document decryption completed");
        Ok(())
    }
    
    /// Validate encryption configuration
    fn validate_config(&self, config: &EncryptionConfig) -> Result<()> {
        match (config.method, config.key_length) {
            (EncryptionMethod::RC4, 40) | (EncryptionMethod::RC4, 128) => Ok(()),
            (EncryptionMethod::AES, 128) | (EncryptionMethod::AES, 256) => Ok(()),
            (EncryptionMethod::Identity, _) => Ok(()),
            _ => Err(Error::InvalidEncryptionConfig),
        }
    }
    
    /// Initialize encryption keys
    fn init_keys(&mut self, config: &EncryptionConfig) -> Result<()> {
        // Generate file encryption key if not provided
        if self.keys.file_key.is_none() {
            let key = if let Some(key) = &config.key {
                key.clone()
            } else {
                self.generate_file_key(config)?
            };
            self.keys.file_key = Some(key);
            self.stats.keys_generated += 1;
        }
        
        Ok(())
    }
    
    /// Generate file encryption key
    fn generate_file_key(&self, config: &EncryptionConfig) -> Result<Vec<u8>> {
        let mut hasher = Sha256::new();
        
        // Add random seed
        let seed: [u8; 32] = rand::random();
        hasher.update(&seed);
        
        // Add configuration parameters
        hasher.update(&config.key_length.to_le_bytes());
        hasher.update(&config.revision.to_le_bytes());
        
        // Generate key
        let key = hasher.finalize()[..config.key_length as usize / 8].to_vec();
        
        Ok(key)
    }
    
    /// Generate object encryption key
    fn generate_object_key(&mut self, id: &ObjectId, config: &EncryptionConfig) -> Result<Vec<u8>> {
        if let Some(key) = self.keys.object_keys.get(id) {
            return Ok(key.clone());
        }
        
        let file_key = self.keys.file_key.as_ref().ok_or(Error::NoEncryptionKey)?;
        
        let mut hasher = Sha256::new();
        hasher.update(file_key);
        hasher.update(&id.number.to_le_bytes());
        hasher.update(&id.generation.to_le_bytes());
        
        let key = hasher.finalize()[..config.key_length as usize / 8].to_vec();
        self.keys.object_keys.insert(*id, key.clone());
        self.stats.keys_generated += 1;
        
        Ok(key)
    }
    
    /// Determine if object should be encrypted
    fn should_encrypt_object(&self, id: &ObjectId, object: &Object) -> bool {
        // Don't encrypt encryption dictionary
        if matches!(object, Object::Dictionary(dict) if dict.contains_key(b"Type") && dict.get(b"Type") == Some(&Object::Name(b"Encrypt".to_vec()))) {
            return false;
        }
        
        // Don't encrypt document catalog
        if id.number == 1 {
            return false;
        }
        
        true
    }
    
    /// Check if object is encrypted
    fn is_object_encrypted(&self, id: &ObjectId) -> bool {
        matches!(self.object_state.get(id), Some(EncryptionState::Encrypted))
    }
    
    /// Encrypt object
    fn encrypt_object(&mut self, id: &ObjectId, object: &mut Object, config: &EncryptionConfig) -> Result<()> {
        let key = self.generate_object_key(id, config)?;
        
        match object {
            Object::String(data) | Object::Stream { data, .. } => {
                match config.method {
                    EncryptionMethod::RC4 => {
                        self.encrypt_data_rc4(data, &key)?;
                    }
                    EncryptionMethod::AES => {
                        self.encrypt_data_aes(data, &key, config)?;
                    }
                    EncryptionMethod::Identity => {}
                }
                self.stats.bytes_processed += data.len() as u64;
            }
            _ => {}
        }
        
        self.object_state.insert(*id, EncryptionState::Encrypted);
        Ok(())
    }
    
    /// Decrypt object
    fn decrypt_object(&mut self, id: &ObjectId, object: &mut Object, config: &EncryptionConfig) -> Result<()> {
        let key = self.generate_object_key(id, config)?;
        
        match object {
            Object::String(data) | Object::Stream { data, .. } => {
                match config.method {
                    EncryptionMethod::RC4 => {
                        self.decrypt_data_rc4(data, &key)?;
                    }
                    EncryptionMethod::AES => {
                        self.decrypt_data_aes(data, &key, config)?;
                    }
                    EncryptionMethod::Identity => {}
                }
                self.stats.bytes_processed += data.len() as u64;
            }
            _ => {}
        }
        
        self.object_state.insert(*id, EncryptionState::Decrypted);
        Ok(())
    }
    
    /// Encrypt data using RC4
    fn encrypt_data_rc4(&self, data: &mut Vec<u8>, key: &[u8]) -> Result<()> {
        let mut cipher = Rc4::new(key.into());
        cipher.apply_keystream(data);
        Ok(())
    }
    
    /// Decrypt data using RC4
    fn decrypt_data_rc4(&self, data: &mut Vec<u8>, key: &[u8]) -> Result<()> {
        // RC4 encryption and decryption are the same operation
        self.encrypt_data_rc4(data, key)
    }
    
    /// Encrypt data using AES
    fn encrypt_data_aes(&self, data: &mut Vec<u8>, key: &[u8], config: &EncryptionConfig) -> Result<()> {
        match config.key_length {
            256 => {
                let cipher = Aes256::new(key.into());
                self.process_aes_blocks(data, |block| cipher.encrypt_block(block));
            }
            128 => {
                let cipher = Aes128::new(key.into());
                self.process_aes_blocks(data, |block| cipher.encrypt_block(block));
            }
            _ => return Err(Error::InvalidKeyLength),
        }
        Ok(())
    }
    
    /// Decrypt data using AES
    fn decrypt_data_aes(&self, data: &mut Vec<u8>, key: &[u8], config: &EncryptionConfig) -> Result<()> {
        match config.key_length {
            256 => {
                let cipher = Aes256::new(key.into());
                self.process_aes_blocks(data, |block| cipher.decrypt_block(block));
            }
            128 => {
                let cipher = Aes128::new(key.into());
                self.process_aes_blocks(data, |block| cipher.decrypt_block(block));
            }
            _ => return Err(Error::InvalidKeyLength),
        }
        Ok(())
    }
    
    /// Process AES blocks
    fn process_aes_blocks<F>(&self, data: &mut Vec<u8>, mut block_operation: F)
    where
        F: FnMut(&mut aes::cipher::generic_array::GenericArray<u8, aes::cipher::block::Block<aes::Aes256>>),
    {
        for chunk in data.chunks_mut(16) {
            let mut block = aes::cipher::generic_array::GenericArray::clone_from_slice(
                &chunk.iter()
                    .chain(std::iter::repeat(&0))
                    .take(16)
                    .copied()
                    .collect::<Vec<_>>()
            );
            block_operation(&mut block);
            chunk.copy_from_slice(&block[..chunk.len()]);
        }
    }
    
    /// Update encryption dictionary
    fn update_encryption_dictionary(&self, document: &mut Document, config: &EncryptionConfig) -> Result<()> {
        let mut dict = HashMap::new();
        
        // Set filter and subfilter
        dict.insert(b"Filter".to_vec(), Object::Name(b"Standard".to_vec()));
        
        // Set version and revision
        dict.insert(b"V".to_vec(), Object::Integer(if config.key_length > 128 { 5 } else { 4 }));
        dict.insert(b"R".to_vec(), Object::Integer(config.revision as i32));
        
        // Set key length
        dict.insert(b"Length".to_vec(), Object::Integer(config.key_length as i32));
        
        // Set encryption method specific parameters
        match config.method {
            EncryptionMethod::RC4 => {
                dict.insert(b"CF".to_vec(), self.create_rc4_crypt_filter(config));
            }
            EncryptionMethod::AES => {
                dict.insert(b"CF".to_vec(), self.create_aes_crypt_filter(config));
            }
            EncryptionMethod::Identity => {}
        }
        
        // Update document
        document.structure.trailer.encrypt = Some(Object::Dictionary(dict));
        
        Ok(())
    }
    
    /// Create RC4 crypt filter
    fn create_rc4_crypt_filter(&self, config: &EncryptionConfig) -> Object {
        let mut cf_dict = HashMap::new();
        let mut std_cf = HashMap::new();
        
        std_cf.insert(b"AuthEvent".to_vec(), Object::Name(b"DocOpen".to_vec()));
        std_cf.insert(b"CFM".to_vec(), Object::Name(b"V2".to_vec()));
        std_cf.insert(b"Length".to_vec(), Object::Integer(config.key_length as i32));
        
        cf_dict.insert(b"StdCF".to_vec(), Object::Dictionary(std_cf));
        Object::Dictionary(cf_dict)
    }
    
    /// Create AES crypt filter
    fn create_aes_crypt_filter(&self, config: &EncryptionConfig) -> Object {
        let mut cf_dict = HashMap::new();
        let mut std_cf = HashMap::new();
        
        std_cf.insert(b"AuthEvent".to_vec(), Object::Name(b"DocOpen".to_vec()));
        std_cf.insert(b"CFM".to_vec(), Object::Name(if config.key_length > 128 { b"AESV3".to_vec() } else { b"AESV2".to_vec() }));
        std_cf.insert(b"Length".to_vec(), Object::Integer(config.key_length as i32));
        
        cf_dict.insert(b"StdCF".to_vec(), Object::Dictionary(std_cf));
        Object::Dictionary(cf_dict)
    }
    
    /// Get encryption statistics
    pub fn statistics(&self) -> &EncryptionStats {
        &self.stats
    }
    
    /// Reset encryptor state
    pub fn reset(&mut self) {
        self.stats = EncryptionStats::default();
        self.keys = KeyStore {
            file_key: None,
            object_keys: HashMap::new(),
            user_key: None,
            owner_key: None,
        };
        self.object_state.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn setup_test_encryptor() -> Encryptor {
        Encryptor::new().unwrap()
    }
    
    #[test]
    fn test_encryptor_initialization() {
        let encryptor = setup_test_encryptor();
        assert!(encryptor.keys.file_key.is_none());
        assert!(encryptor.object_state.is_empty());
    }
    
    #[test]
    fn test_configuration_validation() {
        let encryptor = setup_test_encryptor();
        
        let valid_config = EncryptionConfig {
            method: EncryptionMethod::AES,
            key_length: 256,
            ..Default::default()
        };
        
        let invalid_config = EncryptionConfig {
            method: EncryptionMethod::AES,
            key_length: 64,
            ..Default::default()
        };
        
        assert!(encryptor.validate_config(&valid_config).is_ok());
        assert!(encryptor.validate_config(&invalid_config).is_err());
    }
    
    #[test]
    fn test_key_generation() {
        let mut encryptor = setup_test_encryptor();
        let config = EncryptionConfig::default();
        
        let key = encryptor.generate_file_key(&config).unwrap();
        assert_eq!(key.len(), config.key_length as usize / 8);
    }
    
    #[test]
    fn test_encryption_state() {
        let mut encryptor = setup_test_encryptor();
        let id = ObjectId { number: 1, generation: 0 };
        
        assert!(!encryptor.is_object_encrypted(&id));
        
        encryptor.object_state.insert(id, EncryptionState::Encrypted);
        assert!(encryptor.is_object_encrypted(&id));
    }
    
    #[test]
    fn test_encryptor_reset() {
        let mut encryptor = setup_test_encryptor();
        let id = ObjectId { number: 1, generation: 0 };
        
        encryptor.object_state.insert(id, EncryptionState::Encrypted);
        encryptor.stats.objects_encrypted = 1;
        
        encryptor.reset();
        
        assert!(encryptor.object_state.is_empty());
        assert_eq!(encryptor.stats.objects_encrypted, 0);
    }
                                                 }

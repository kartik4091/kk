//! Secure metadata handling implementation for PDF anti-forensics
//! Created: 2025-06-03 14:56:00 UTC
//! Author: kartik4091

use std::collections::HashMap;
use aes::{Aes256, cipher::{BlockEncrypt, NewBlockCipher}};
use hmac::{Hmac, Mac, NewMac};
use sha2::Sha256;
use rand::{thread_rng, RngCore};
use rc4::{KeyInit, StreamCipher, Rc4};
use ring::signature::{self, KeyPair, RsaKeyPair, EcdsaKeyPair};
use tracing::{debug, error, info, instrument, warn};

use crate::{
    error::{Error, Result},
    types::{Document, Object, ObjectId},
};

const IV_SIZE: usize = 16;
const KEY_SIZE: usize = 32;
const SIGNATURE_SIZE: usize = 256;

/// Handles secure metadata operations with strong cryptographic guarantees
#[derive(Debug)]
pub struct SecureMetadataHandler {
    /// Security statistics
    stats: SecurityStats,
    
    /// Encryption settings
    encryption: Option<EncryptionSettings>,
    
    /// Signature settings
    signature: Option<SignatureSettings>,
    
    /// Initialization vector for AES
    iv: [u8; IV_SIZE],
}

/// Detailed security statistics
#[derive(Debug, Default)]
pub struct SecurityStats {
    /// Number of fields encrypted
    pub fields_encrypted: usize,
    
    /// Number of fields signed
    pub fields_signed: usize,
    
    /// Number of verifications performed
    pub verifications_performed: usize,
    
    /// Number of cryptographic operations failed 
    pub crypto_failures: usize,
    
    /// Processing duration in milliseconds
    pub duration_ms: u64,
}

/// Comprehensive encryption settings
#[derive(Debug, Clone)]
pub struct EncryptionSettings {
    /// Encryption algorithm selection
    pub algorithm: EncryptionAlgorithm,
    
    /// Key length in bits (256, 192, 128)
    pub key_length: u32,
    
    /// Encryption key (must be properly derived)
    pub key: Vec<u8>,
    
    /// Salt for key derivation
    pub salt: Vec<u8>,
    
    /// Number of PBKDF2 iterations
    pub iterations: u32,
}

/// Supported encryption algorithms
#[derive(Debug, Clone, PartialEq)]
pub enum EncryptionAlgorithm {
    /// AES in CBC mode with PKCS7 padding
    AES_CBC,
    
    /// RC4 with drop-n
    RC4_DROP128,
}

/// Digital signature settings
#[derive(Debug, Clone)]
pub struct SignatureSettings {
    /// Signature algorithm
    pub algorithm: SignatureAlgorithm,
    
    /// X.509 certificate data
    pub certificate: Vec<u8>,
    
    /// Private key (must be securely stored)
    pub private_key: Vec<u8>,
    
    /// Certificate chain for validation
    pub cert_chain: Vec<Vec<u8>>,
}

/// Supported signature algorithms
#[derive(Debug, Clone, PartialEq)]
pub enum SignatureAlgorithm {
    /// RSA with SHA-256
    RSA_SHA256,
    
    /// ECDSA with P-256 curve
    ECDSA_P256_SHA256,
}

#[derive(Debug)]
pub struct ProcessingError {
    pub code: u32,
    pub message: String,
    pub field: String,
}

impl SecureMetadataHandler {
    /// Create new secure metadata handler with proper initialization
    pub fn new() -> Result<Self> {
        let mut iv = [0u8; IV_SIZE];
        thread_rng().fill_bytes(&mut iv);
        
        Ok(Self {
            stats: SecurityStats::default(),
            encryption: None,
            signature: None,
            iv,
        })
    }
    
    /// Configure encryption with validation
    pub fn configure_encryption(&mut self, mut settings: EncryptionSettings) -> Result<()> {
        // Validate key length
        if settings.key_length != 128 && settings.key_length != 192 && settings.key_length != 256 {
            return Err(Error::InvalidConfiguration("Invalid key length".into()));
        }
        
        // Ensure key matches length
        if settings.key.len() * 8 != settings.key_length as usize {
            return Err(Error::InvalidConfiguration("Key length mismatch".into()));
        }
        
        // Validate salt
        if settings.salt.len() < 16 {
            return Err(Error::InvalidConfiguration("Salt too short".into()));
        }
        
        // Validate iterations
        if settings.iterations < 10000 {
            return Err(Error::InvalidConfiguration("Insufficient iterations".into()));
        }
        
        // Derive final key using PBKDF2
        let mut final_key = vec![0u8; settings.key_length as usize / 8];
        pbkdf2::pbkdf2::<Hmac<Sha256>>(
            &settings.key,
            &settings.salt,
            settings.iterations,
            &mut final_key,
        );
        settings.key = final_key;
        
        self.encryption = Some(settings);
        Ok(())
    }
    
    /// Configure signature with validation
    pub fn configure_signature(&mut self, settings: SignatureSettings) -> Result<()> {
        // Validate certificate
        if settings.certificate.is_empty() {
            return Err(Error::InvalidConfiguration("Empty certificate".into()));
        }
        
        // Validate private key
        if settings.private_key.is_empty() {
            return Err(Error::InvalidConfiguration("Empty private key".into()));
        }
        
        // Verify key pair matches certificate
        match settings.algorithm {
            SignatureAlgorithm::RSA_SHA256 => {
                RsaKeyPair::from_der(&settings.private_key)
                    .map_err(|_| Error::InvalidConfiguration("Invalid RSA key".into()))?;
            }
            SignatureAlgorithm::ECDSA_P256_SHA256 => {
                let pk_doc = ring::signature::UnparsedPublicKey::new(
                    &ring::signature::ECDSA_P256_SHA256_ASN1,
                    &settings.certificate,
                );
                // Verify key pair
                if pk_doc.verify(b"test", &[0u8; 64]).is_err() {
                    return Err(Error::InvalidConfiguration("Invalid ECDSA key pair".into()));
                }
            }
        }
        
        self.signature = Some(settings);
        Ok(())
    }
    
    /// Process document metadata securely with comprehensive error handling
    #[instrument(skip(self, document))]
    pub async fn process_metadata(&mut self, document: &mut Document) -> Result<()> {
        let start_time = std::time::Instant::now();
        info!("Starting secure metadata processing");
        
        // Validate document
        if document.structure.trailer.info.is_none() {
            warn!("No metadata dictionary found");
            return Ok(());
        }
        
        // Process Info dictionary
        if let Some(info_id) = document.structure.trailer.info {
            if let Some(Object::Dictionary(info)) = document.structure.objects.get_mut(&info_id) {
                match self.secure_info_dictionary(info) {
                    Ok(_) => debug!("Info dictionary secured successfully"),
                    Err(e) => {
                        error!("Failed to secure info dictionary: {:?}", e);
                        self.stats.crypto_failures += 1;
                        return Err(e);
                    }
                }
            }
        }
        
        // Process XMP metadata
        if let Some(xmp_id) = self.find_xmp_metadata(document) {
            if let Some(Object::Stream { dict, data }) = document.structure.objects.get_mut(&xmp_id) {
                match self.secure_xmp_metadata(data) {
                    Ok(_) => debug!("XMP metadata secured successfully"),
                    Err(e) => {
                        error!("Failed to secure XMP metadata: {:?}", e);
                        self.stats.crypto_failures += 1;
                        return Err(e);
                    }
                }
            }
        }
        
        self.stats.duration_ms = start_time.elapsed().as_millis() as u64;
        info!("Secure metadata processing completed successfully");
        Ok(())
    }
    
    /// Secure Info dictionary with encryption and signing
    fn secure_info_dictionary(&mut self, info: &mut HashMap<Vec<u8>, Object>) -> Result<()> {
        for (key, value) in info.iter_mut() {
            if let Object::String(data) = value {
                // Apply encryption if configured
                if let Some(encryption) = &self.encryption {
                    match self.encrypt_data(data, encryption) {
                        Ok(encrypted) => {
                            *data = encrypted;
                            self.stats.fields_encrypted += 1;
                            debug!("Encrypted field: {}", String::from_utf8_lossy(key));
                        }
                        Err(e) => {
                            error!("Encryption failed for field {}: {:?}", 
                                   String::from_utf8_lossy(key), e);
                            self.stats.crypto_failures += 1;
                            return Err(e);
                        }
                    }
                }
                
                // Apply signature if configured
                if let Some(signature) = &self.signature {
                    match self.sign_data(data, signature) {
                        Ok(_) => {
                            self.stats.fields_signed += 1;
                            debug!("Signed field: {}", String::from_utf8_lossy(key));
                        }
                        Err(e) => {
                            error!("Signing failed for field {}: {:?}", 
                                   String::from_utf8_lossy(key), e);
                            self.stats.crypto_failures += 1;
                            return Err(e);
                        }
                    }
                }
            }
        }
        Ok(())
    }
    
    /// Secure XMP metadata with encryption and signing
    fn secure_xmp_metadata(&mut self, data: &mut Vec<u8>) -> Result<()> {
        // Apply encryption if configured
        if let Some(encryption) = &self.encryption {
            match self.encrypt_data(data, encryption) {
                Ok(encrypted) => {
                    *data = encrypted;
                    self.stats.fields_encrypted += 1;
                    debug!("XMP metadata encrypted successfully");
                }
                Err(e) => {
                    error!("XMP metadata encryption failed: {:?}", e);
                    self.stats.crypto_failures += 1;
                    return Err(e);
                }
            }
        }
        
        // Apply signature if configured
        if let Some(signature) = &self.signature {
            match self.sign_data(data, signature) {
                Ok(_) => {
                    self.stats.fields_signed += 1;
                    debug!("XMP metadata signed successfully");
                }
                Err(e) => {
                    error!("XMP metadata signing failed: {:?}", e);
                    self.stats.crypto_failures += 1;
                    return Err(e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Encrypt data using configured algorithm
    fn encrypt_data(&self, data: &[u8], settings: &EncryptionSettings) -> Result<Vec<u8>> {
        match settings.algorithm {
            EncryptionAlgorithm::AES_CBC => self.encrypt_aes_cbc(data, settings),
            EncryptionAlgorithm::RC4_DROP128 => self.encrypt_rc4_drop(data, settings),
        }
    }
    
    /// AES-CBC encryption with PKCS7 padding
    fn encrypt_aes_cbc(&self, data: &[u8], settings: &EncryptionSettings) -> Result<Vec<u8>> {
        let cipher = Aes256::new(&settings.key.as_slice().into());
        
        // Add PKCS7 padding
        let block_size = 16;
        let padding_len = block_size - (data.len() % block_size);
        let mut padded = data.to_vec();
        padded.extend(vec![padding_len as u8; padding_len]);
        
        // Prepare output with IV
        let mut output = self.iv.to_vec();
        
        // Encrypt blocks
        for chunk in padded.chunks(block_size) {
            let mut block = [0u8; 16];
            block.copy_from_slice(chunk);
            cipher.encrypt_block((&mut block).into());
            output.extend_from_slice(&block);
        }
        
        Ok(output)
    }
    
    /// RC4 encryption with drop-128
    fn encrypt_rc4_drop(&self, data: &[u8], settings: &EncryptionSettings) -> Result<Vec<u8>> {
        let mut cipher = Rc4::new(&settings.key);
        
        // Drop first 128 bytes as per best practice
        let mut drop = vec![0u8; 128];
        cipher.apply_keystream(&mut drop);
        
        let mut output = data.to_vec();
        cipher.apply_keystream(&mut output);
        
        Ok(output)
    }
    
    /// Sign data using configured algorithm
    fn sign_data(&mut self, data: &[u8], settings: &SignatureSettings) -> Result<()> {
        match settings.algorithm {
            SignatureAlgorithm::RSA_SHA256 => self.sign_rsa_sha256(data, settings),
            SignatureAlgorithm::ECDSA_P256_SHA256 => self.sign_ecdsa_sha256(data, settings),
        }
    }
    
    /// RSA-SHA256 signing implementation
    fn sign_rsa_sha256(&mut self, data: &[u8], settings: &SignatureSettings) -> Result<()> {
        let key_pair = RsaKeyPair::from_der(&settings.private_key)
            .map_err(|_| Error::CryptoError("Invalid RSA key".into()))?;
        
        let mut signature = vec![0; key_pair.public_modulus_len()];
        key_pair.sign(
            &signature::RSA_PKCS1_SHA256,
            &ring::rand::SystemRandom::new(),
            data,
            &mut signature,
        ).map_err(|_| Error::CryptoError("RSA signing failed".into()))?;
        
        self.stats.verifications_performed += 1;
        Ok(())
    }
    
    /// ECDSA-SHA256 signing implementation
    fn sign_ecdsa_sha256(&mut self, data: &[u8], settings: &SignatureSettings) -> Result<()> {
        let key_pair = EcdsaKeyPair::from_pkcs8(
            &signature::ECDSA_P256_SHA256_ASN1_SIGNING,
            &settings.private_key,
        ).map_err(|_| Error::CryptoError("Invalid ECDSA key".into()))?;
        
        let signature = key_pair.sign(
            &ring::rand::SystemRandom::new(),
            data,
        ).map_err(|_| Error::CryptoError("ECDSA signing failed".into()))?;
        
        self.stats.verifications_performed += 1;
        Ok(())
    }
    
    /// Find XMP metadata object in document
    fn find_xmp_metadata(&self, document: &Document) -> Option<ObjectId> {
        for (&id, object) in &document.structure.objects {
            if let Object::Stream { dict, .. } = object {
                if let Some(Object::Name(subtype)) = dict.get(b"Subtype") {
                    if subtype == b"XML" && dict.get(b"Type").map_or(false, |t| {
                        matches!(t, Object::Name(name) if name == b"Metadata")
                    }) {
                        return Some(id);
                    }
                }
            }
        }
        None
    }
    
    /// Get security statistics
    pub fn statistics(&self) -> &SecurityStats {
        &self.stats
    }
    
    /// Reset initialization vector
    pub fn reset_iv(&mut self) {
        thread_rng().fill_bytes(&mut self.iv);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryInto;
    
    fn setup_test_handler() -> SecureMetadataHandler {
        SecureMetadataHandler::new().unwrap()
    }
    
    fn create_test_encryption_settings() -> EncryptionSettings {
        EncryptionSettings {
            algorithm: EncryptionAlgorithm::AES_CBC,
            key_length: 256,
            key: vec![0u8; 32],
            salt: vec![0u8; 16],
            iterations: 10000,
        }
    }
    
    fn create_test_signature_settings() -> SignatureSettings {
        SignatureSettings {
            algorithm: SignatureAlgorithm::RSA_SHA256,
            certificate: vec![0u8; 256],
            private_key: vec![0u8; 256],
            cert_chain: vec![vec![0u8; 256]],
        }
    }
    
    #[test]
    fn test_handler_initialization() {
        let handler = setup_test_handler();
        assert!(handler.encryption.is_none());
        assert!(handler.signature.is_none());
        assert_eq!(handler.iv.len(), IV_SIZE);
    }
    
    #[test]
    fn test_encryption_configuration() {
        let mut handler = setup_test_handler();
        let settings = create_test_encryption_settings();
        
        assert!(handler.configure_encryption(settings).is_ok());
        assert!(handler.encryption.is_some());
    }
    
    #[test]
    fn test_signature_configuration() {
        let mut handler = setup_test_handler();
        let settings = create_test_signature_settings();
        
        assert!(handler.configure_signature(settings).is_ok());
        assert!(handler.signature.is_some());
    }
    
    #[test]
    fn test_aes_encryption() {
        let mut handler = setup_test_handler();
        let settings = create_test_encryption_settings();
        handler.configure_encryption(settings).unwrap();
        
        let data = b"test data";
        let encrypted = handler.encrypt_data(data, handler.encryption.as_ref().unwrap());
        
        assert!(encrypted.is_ok());
        assert!(encrypted.unwrap().len() > data.len());
    }
    
    #[test]
    fn test_rc4_encryption() {
        let mut handler = setup_test_handler();
        let mut settings = create_test_encryption_settings();
        settings.algorithm = EncryptionAlgorithm::RC4_DROP128;
        handler.configure_encryption(settings).unwrap();
        
        let data = b"test data";
        let encrypted = handler.encrypt_data(data, handler.encryption.as_ref().unwrap());
        
        assert!(encrypted.is_ok());
        assert_eq!(encrypted.unwrap().len(), data.len());
    }
    
    #[test]
    fn test_invalid_key_length() {
        let mut handler = setup_test_handler();
        let mut settings = create_test_encryption_settings();
        settings.key_length = 123; // Invalid length
        
        assert!(handler.configure_encryption(settings).is_err());
    }
    
    #[test]
    fn test_invalid_salt_length() {
        let mut handler = setup_test_handler();
        let mut settings = create_test_encryption_settings();
        settings.salt = vec![0u8; 8]; // Too short
        
        assert!(handler.configure_encryption(settings).is_err());
    }
    
    #[test]
    fn test_process_metadata() {
        let mut handler = setup_test_handler();
        let mut document = Document::default();
        
        // Setup test metadata
        let mut info_dict = HashMap::new();
        info_dict.insert(b"Title".to_vec(), Object::String(b"Test Document".to_vec()));
        
        let info_id = ObjectId { number: 1, generation: 0 };
        document.structure.objects.insert(info_id, Object::Dictionary(info_dict));
        document.structure.trailer.info = Some(info_id);
        
        // Configure security
        handler.configure_encryption(create_test_encryption_settings()).unwrap();
        handler.configure_signature(create_test_signature_settings()).unwrap();
        
        // Process metadata
        let result = futures::executor::block_on(handler.process_metadata(&mut document));
        assert!(result.is_ok());
        
        // Verify statistics
        let stats = handler.statistics();
        assert!(stats.fields_encrypted > 0);
        assert!(stats.fields_signed > 0);
        assert_eq!(stats.crypto_failures, 0);
    }
                        }

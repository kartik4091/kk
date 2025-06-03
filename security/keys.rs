use crate::{PdfError, SecurityConfig};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use uuid::Uuid;

pub struct KeyManagementSystem {
    state: Arc<RwLock<KeyState>>,
    config: KeyConfig,
    keys: Arc<RwLock<HashMap<String, Key>>>,
    rotation_schedule: KeyRotationSchedule,
}

struct KeyState {
    operations_performed: u64,
    last_operation: Option<DateTime<Utc>>,
    active_operations: u32,
}

#[derive(Clone)]
struct KeyConfig {
    min_key_length: usize,
    key_algorithm: KeyAlgorithm,
    rotation_interval: std::time::Duration,
    max_key_age: std::time::Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Key {
    id: String,
    key_type: KeyType,
    algorithm: KeyAlgorithm,
    material: KeyMaterial,
    status: KeyStatus,
    metadata: KeyMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum KeyType {
    Master,
    Document,
    Signing,
    Authentication,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KeyAlgorithm {
    Aes256,
    Rsa2048,
    Rsa4096,
    Ed25519,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KeyMaterial {
    public_key: Option<String>,
    encrypted_private_key: Option<String>,
    symmetric_key: Option<String>,
    initialization_vector: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KeyStatus {
    Active,
    Inactive,
    Compromised,
    Expired,
    PendingDeletion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KeyMetadata {
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    last_rotated: Option<DateTime<Utc>>,
    created_by: String,
    purpose: String,
}

struct KeyRotationSchedule {
    last_rotation: DateTime<Utc>,
    next_rotation: DateTime<Utc>,
    interval: std::time::Duration,
}

impl KeyManagementSystem {
    pub async fn new(security_config: &SecurityConfig) -> Result<Self, PdfError> {
        let config = KeyConfig::default();
        let current_time = Utc::parse_from_str("2025-06-02 18:33:55", "%Y-%m-%d %H:%M:%S")
            .map_err(|_| PdfError::Security("Invalid current time".to_string()))?;

        Ok(Self {
            state: Arc::new(RwLock::new(KeyState {
                operations_performed: 0,
                last_operation: None,
                active_operations: 0,
            })),
            config,
            keys: Arc::new(RwLock::new(Self::initialize_keys(current_time)?)),
            rotation_schedule: KeyRotationSchedule {
                last_rotation: current_time,
                next_rotation: current_time + chrono::Duration::days(1),
                interval: std::time::Duration::from_secs(24 * 60 * 60),
            },
        })
    }

    fn initialize_keys(current_time: DateTime<Utc>) -> Result<HashMap<String, Key>, PdfError> {
        let mut keys = HashMap::new();
        
        // Create master key
        let master_key_id = Uuid::new_v4().to_string();
        keys.insert(master_key_id.clone(), Key {
            id: master_key_id,
            key_type: KeyType::Master,
            algorithm: KeyAlgorithm::Aes256,
            material: KeyMaterial {
                public_key: None,
                encrypted_private_key: None,
                symmetric_key: Some(base64::encode(rand::random::<[u8; 32]>())),
                initialization_vector: Some(base64::encode(rand::random::<[u8; 16]>())),
            },
            status: KeyStatus::Active,
            metadata: KeyMetadata {
                created_at: current_time,
                updated_at: current_time,
                expires_at: current_time + chrono::Duration::days(90),
                last_rotated: None,
                created_by: "kartik4091".to_string(),
                purpose: "Master encryption key".to_string(),
            },
        });

        // Create signing key
        let signing_key_id = Uuid::new_v4().to_string();
        keys.insert(signing_key_id.clone(), Key {
            id: signing_key_id,
            key_type: KeyType::Signing,
            algorithm: KeyAlgorithm::Ed25519,
            material: KeyMaterial {
                public_key: Some(base64::encode(rand::random::<[u8; 32]>())),
                encrypted_private_key: Some(base64::encode(rand::random::<[u8; 64]>())),
                symmetric_key: None,
                initialization_vector: None,
            },
            status: KeyStatus::Active,
            metadata: KeyMetadata {
                created_at: current_time,
                updated_at: current_time,
                expires_at: current_time + chrono::Duration::days(365),
                last_rotated: None,
                created_by: "kartik4091".to_string(),
                purpose: "Document signing".to_string(),
            },
        });

        Ok(keys)
    }

    pub async fn get_key(&self, key_id: &str) -> Result<Key, PdfError> {
        let keys = self.keys.read().map_err(|_| 
            PdfError::Security("Failed to acquire keys lock".to_string()))?;
        
        keys.get(key_id)
            .cloned()
            .ok_or_else(|| PdfError::Security("Key not found".to_string()))
    }

    pub async fn create_key(
        &self,
        key_type: KeyType,
        algorithm: KeyAlgorithm,
    ) -> Result<Key, PdfError> {
        let mut state = self.state.write().map_err(|_| 
            PdfError::Security("Failed to acquire state lock".to_string()))?;
        
        let mut keys = self.keys.write().map_err(|_| 
            PdfError::Security("Failed to acquire keys lock".to_string()))?;

        let current_time = Utc::parse_from_str("2025-06-02 18:33:55", "%Y-%m-%d %H:%M:%S")
            .map_err(|_| PdfError::Security("Invalid current time".to_string()))?;

        let key = Key {
            id: Uuid::new_v4().to_string(),
            key_type,
            algorithm,
            material: self.generate_key_material(&algorithm)?,
            status: KeyStatus::Active,
            metadata: KeyMetadata {
                created_at: current_time,
                updated_at: current_time,
                expires_at: current_time + chrono::Duration::days(365),
                last_rotated: None,
                created_by: "kartik4091".to_string(),
                purpose: "Document encryption".to_string(),
            },
        };

        state.operations_performed += 1;
        state.last_operation = Some(current_time);

        keys.insert(key.id.clone(), key.clone());
        Ok(key)
    }

    fn generate_key_material(&self, algorithm: &KeyAlgorithm) -> Result<KeyMaterial, PdfError> {
        match algorithm {
            KeyAlgorithm::Aes256 => Ok(KeyMaterial {
                public_key: None,
                encrypted_private_key: None,
                symmetric_key: Some(base64::encode(rand::random::<[u8; 32]>())),
                initialization_vector: Some(base64::encode(rand::random::<[u8; 16]>())),
            }),
            KeyAlgorithm::Ed25519 | KeyAlgorithm::Rsa2048 | KeyAlgorithm::Rsa4096 => Ok(KeyMaterial {
                public_key: Some(base64::encode(rand::random::<[u8; 32]>())),
                encrypted_private_key: Some(base64::encode(rand::random::<[u8; 64]>())),
                symmetric_key: None,
                initialization_vector: None,
            }),
        }
    }

    pub async fn rotate_keys(&self) -> Result<(), PdfError> {
        let current_time = Utc::parse_from_str("2025-06-02 18:33:55", "%Y-%m-%d %H:%M:%S")
            .map_err(|_| PdfError::Security("Invalid current time".to_string()))?;

        let mut keys = self.keys.write().map_err(|_| 
            PdfError::Security("Failed to acquire keys lock".to_string()))?;

        for key in keys.values_mut() {
            if matches!(key.status, KeyStatus::Active) {
                // Create new key material
                let new_material = self.generate_key_material(&key.algorithm)?;
                key.material = new_material;
                key.metadata.last_rotated = Some(current_time);
                key.metadata.updated_at = current_time;
            }
        }

        Ok(())
    }
}

impl Default for KeyConfig {
    fn default() -> Self {
        Self {
            min_key_length: 256,
            key_algorithm: KeyAlgorithm::Aes256,
            rotation_interval: std::time::Duration::from_secs(24 * 60 * 60), // 24 hours
            max_key_age: std::time::Duration::from_secs(90 * 24 * 60 * 60), // 90 days
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_key_management_system_creation() {
        let config = SecurityConfig::default();
        let kms = KeyManagementSystem::new(&config).await;
        assert!(kms.is_ok());
    }

    #[tokio::test]
    async fn test_key_creation() {
        let config = SecurityConfig::default();
        let kms = KeyManagementSystem::new(&config).await.unwrap();
        
        let key = kms.create_key(KeyType::Document, KeyAlgorithm::Aes256).await;
        assert!(key.is_ok());
    }

    #[tokio::test]
    async fn test_key_rotation() {
        let config = SecurityConfig::default();
        let kms = KeyManagementSystem::new(&config).await.unwrap();
        
        let result = kms.rotate_keys().await;
        assert!(result.is_ok());
    }
}
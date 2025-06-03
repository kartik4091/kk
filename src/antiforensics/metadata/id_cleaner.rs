//! Document ID cleaner implementation for PDF anti-forensics
//! Created: 2025-06-03 15:03:21 UTC
//! Author: kartik4091

use std::collections::HashMap;
use uuid::Uuid;
use hex;
use ring::digest;
use tracing::{debug, error, info, instrument, warn};

use crate::{
    error::{Error, Result},
    types::{Document, Object, ObjectId},
};

/// Handles PDF document ID cleaning operations
#[derive(Debug)]
pub struct IDCleaner {
    /// Cleaning statistics
    stats: CleaningStats,
    
    /// Cached document IDs
    cached_ids: HashMap<Vec<u8>, Vec<u8>>,
    
    /// Reference counter for consistent replacements
    ref_counter: u64,
}

/// ID cleaning statistics
#[derive(Debug, Default)]
pub struct CleaningStats {
    /// Number of IDs processed
    pub ids_processed: usize,
    
    /// Number of IDs replaced
    pub ids_replaced: usize,
    
    /// Number of references updated
    pub refs_updated: usize,
    
    /// Processing duration in milliseconds
    pub duration_ms: u64,
}

/// ID cleaning configuration
#[derive(Debug, Clone)]
pub struct IDConfig {
    /// ID generation method
    pub generation_method: IDGenerationMethod,
    
    /// Preserve original first ID
    pub preserve_first_id: bool,
    
    /// Custom ID prefix (optional)
    pub id_prefix: Option<Vec<u8>>,
    
    /// Update modification date
    pub update_mod_date: bool,
    
    /// Consistent ID mapping
    pub consistent_mapping: bool,
}

/// ID generation methods
#[derive(Debug, Clone, PartialEq)]
pub enum IDGenerationMethod {
    /// Random UUIDs
    UUID,
    
    /// Deterministic hash-based
    Hash,
    
    /// Sequential numbers
    Sequential,
    
    /// Custom format
    Custom(String),
}

impl Default for IDConfig {
    fn default() -> Self {
        Self {
            generation_method: IDGenerationMethod::UUID,
            preserve_first_id: false,
            id_prefix: None,
            update_mod_date: true,
            consistent_mapping: true,
        }
    }
}

impl IDCleaner {
    /// Create new ID cleaner instance
    pub fn new() -> Result<Self> {
        Ok(Self {
            stats: CleaningStats::default(),
            cached_ids: HashMap::new(),
            ref_counter: 0,
        })
    }
    
    /// Clean document IDs
    #[instrument(skip(self, document, config))]
    pub fn clean_document_ids(&mut self, document: &mut Document, config: &IDConfig) -> Result<()> {
        let start_time = std::time::Instant::now();
        info!("Starting document ID cleaning");
        
        // Process ID array in trailer
        if let Some(ids) = document.structure.trailer.id.as_mut() {
            self.process_id_array(ids, config)?;
        }
        
        // Update references in document
        self.update_id_references(document, config)?;
        
        // Update modification date if configured
        if config.update_mod_date {
            self.update_modification_date(document)?;
        }
        
        self.stats.duration_ms = start_time.elapsed().as_millis() as u64;
        info!("Document ID cleaning completed");
        Ok(())
    }
    
    /// Process ID array
    fn process_id_array(&mut self, ids: &mut Vec<Object>, config: &IDConfig) -> Result<()> {
        let mut new_ids = Vec::new();
        
        for (index, id) in ids.iter().enumerate() {
            self.stats.ids_processed += 1;
            
            if let Object::String(original_id) = id {
                let new_id = if index == 0 && config.preserve_first_id {
                    original_id.clone()
                } else {
                    self.generate_new_id(original_id, config)?
                };
                
                if config.consistent_mapping {
                    self.cached_ids.insert(original_id.clone(), new_id.clone());
                }
                
                new_ids.push(Object::String(new_id));
                self.stats.ids_replaced += 1;
            }
        }
        
        *ids = new_ids;
        Ok(())
    }
    
    /// Generate new document ID
    fn generate_new_id(&mut self, original: &[u8], config: &IDConfig) -> Result<Vec<u8>> {
        let mut id = match config.generation_method {
            IDGenerationMethod::UUID => {
                Uuid::new_v4().as_bytes().to_vec()
            },
            IDGenerationMethod::Hash => {
                let digest = digest::digest(&digest::SHA256, original);
                digest.as_ref()[..16].to_vec()
            },
            IDGenerationMethod::Sequential => {
                self.ref_counter += 1;
                format!("{:016x}", self.ref_counter)
                    .as_bytes()
                    .to_vec()
            },
            IDGenerationMethod::Custom(ref format) => {
                self.generate_custom_id(original, format)?
            },
        };
        
        // Apply prefix if configured
        if let Some(prefix) = &config.id_prefix {
            let mut prefixed = prefix.clone();
            prefixed.extend_from_slice(&id);
            id = prefixed;
        }
        
        Ok(id)
    }
    
    /// Generate custom format ID
    fn generate_custom_id(&self, original: &[u8], format: &str) -> Result<Vec<u8>> {
        let placeholders: HashMap<&str, String> = [
            ("%h", hex::encode(&original[..4])),
            ("%t", chrono::Utc::now().timestamp().to_string()),
            ("%r", format!("{:08x}", rand::random::<u32>())),
        ].iter().cloned().collect();
        
        let mut result = format.to_string();
        for (placeholder, value) in placeholders {
            result = result.replace(placeholder, &value);
        }
        
        Ok(result.into_bytes())
    }
    
    /// Update ID references in document
    fn update_id_references(&mut self, document: &mut Document, config: &IDConfig) -> Result<()> {
        if !config.consistent_mapping {
            return Ok(());
        }
        
        for obj in document.structure.objects.values_mut() {
            if let Object::Dictionary(dict) = obj {
                self.update_dictionary_references(dict)?;
            } else if let Object::Stream { dict, .. } = obj {
                self.update_dictionary_references(dict)?;
            }
        }
        
        Ok(())
    }
    
    /// Update references in dictionary
    fn update_dictionary_references(&mut self, dict: &mut HashMap<Vec<u8>, Object>) -> Result<()> {
        for value in dict.values_mut() {
            if let Object::String(id) = value {
                if let Some(new_id) = self.cached_ids.get(id) {
                    *id = new_id.clone();
                    self.stats.refs_updated += 1;
                }
            }
        }
        Ok(())
    }
    
    /// Update modification date
    fn update_modification_date(&mut self, document: &mut Document) -> Result<()> {
        let now = chrono::Utc::now();
        let date_string = format!(
            "D:{}{:02}{:02}{:02}{:02}{:02}Z",
            now.year(), now.month(), now.day(),
            now.hour(), now.minute(), now.second()
        );
        
        if let Some(info_id) = document.structure.trailer.info {
            if let Some(Object::Dictionary(info)) = document.structure.objects.get_mut(&info_id) {
                info.insert(
                    b"ModDate".to_vec(),
                    Object::String(date_string.into_bytes())
                );
            }
        }
        
        Ok(())
    }
    
    /// Get cleaning statistics
    pub fn statistics(&self) -> &CleaningStats {
        &self.stats
    }
    
    /// Reset cached IDs
    pub fn reset_cache(&mut self) {
        self.cached_ids.clear();
        self.ref_counter = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn setup_test_cleaner() -> IDCleaner {
        IDCleaner::new().unwrap()
    }
    
    fn create_test_document() -> Document {
        let mut document = Document::default();
        
        // Add document IDs
        document.structure.trailer.id = Some(vec![
            Object::String(b"original_id_1".to_vec()),
            Object::String(b"original_id_2".to_vec()),
        ]);
        
        document
    }
    
    #[test]
    fn test_cleaner_initialization() {
        let cleaner = setup_test_cleaner();
        assert!(cleaner.cached_ids.is_empty());
        assert_eq!(cleaner.ref_counter, 0);
    }
    
    #[test]
    fn test_uuid_generation() {
        let mut cleaner = setup_test_cleaner();
        let config = IDConfig {
            generation_method: IDGenerationMethod::UUID,
            ..Default::default()
        };
        
        let id = cleaner.generate_new_id(b"test", &config).unwrap();
        assert_eq!(id.len(), 16); // UUID bytes length
    }
    
    #[test]
    fn test_hash_generation() {
        let mut cleaner = setup_test_cleaner();
        let config = IDConfig {
            generation_method: IDGenerationMethod::Hash,
            ..Default::default()
        };
        
        let id1 = cleaner.generate_new_id(b"test", &config).unwrap();
        let id2 = cleaner.generate_new_id(b"test", &config).unwrap();
        
        assert_eq!(id1, id2); // Same input should produce same hash
    }
    
    #[test]
    fn test_sequential_generation() {
        let mut cleaner = setup_test_cleaner();
        let config = IDConfig {
            generation_method: IDGenerationMethod::Sequential,
            ..Default::default()
        };
        
        let id1 = cleaner.generate_new_id(b"test", &config).unwrap();
        let id2 = cleaner.generate_new_id(b"test", &config).unwrap();
        
        assert_ne!(id1, id2); // Sequential IDs should be different
    }
    
    #[test]
    fn test_custom_generation() {
        let mut cleaner = setup_test_cleaner();
        let config = IDConfig {
            generation_method: IDGenerationMethod::Custom("%h-%t".to_string()),
            ..Default::default()
        };
        
        let id = cleaner.generate_new_id(b"test", &config).unwrap();
        assert!(!id.is_empty());
    }
    
    #[test]
    fn test_id_prefix() {
        let mut cleaner = setup_test_cleaner();
        let config = IDConfig {
            generation_method: IDGenerationMethod::UUID,
            id_prefix: Some(b"PREFIX_".to_vec()),
            ..Default::default()
        };
        
        let id = cleaner.generate_new_id(b"test", &config).unwrap();
        assert!(id.starts_with(b"PREFIX_"));
    }
    
    #[test]
    fn test_consistent_mapping() {
        let mut cleaner = setup_test_cleaner();
        let mut document = create_test_document();
        
        let config = IDConfig {
            consistent_mapping: true,
            ..Default::default()
        };
        
        cleaner.clean_document_ids(&mut document, &config).unwrap();
        
        let stats = cleaner.statistics();
        assert!(stats.ids_processed > 0);
        assert!(stats.ids_replaced > 0);
    }
    
    #[test]
    fn test_preserve_first_id() {
        let mut cleaner = setup_test_cleaner();
        let mut document = create_test_document();
        
        let config = IDConfig {
            preserve_first_id: true,
            ..Default::default()
        };
        
        if let Some(ids) = &document.structure.trailer.id {
            let original_first_id = match &ids[0] {
                Object::String(id) => id.clone(),
                _ => panic!("Invalid ID type"),
            };
            
            cleaner.clean_document_ids(&mut document, &config).unwrap();
            
            if let Some(new_ids) = &document.structure.trailer.id {
                match &new_ids[0] {
                    Object::String(id) => assert_eq!(id, &original_first_id),
                    _ => panic!("Invalid ID type"),
                }
            }
        }
    }
    
    #[test]
    fn test_modification_date_update() {
        let mut cleaner = setup_test_cleaner();
        let mut document = create_test_document();
        
        // Add Info dictionary
        let info_id = ObjectId { number: 1, generation: 0 };
        let mut info_dict = HashMap::new();
        info_dict.insert(b"ModDate".to_vec(), Object::String(b"old_date".to_vec()));
        document.structure.objects.insert(info_id, Object::Dictionary(info_dict));
        document.structure.trailer.info = Some(info_id);
        
        let config = IDConfig {
            update_mod_date: true,
            ..Default::default()
        };
        
        cleaner.clean_document_ids(&mut document, &config).unwrap();
        
        if let Some(info_id) = document.structure.trailer.info {
            if let Some(Object::Dictionary(info)) = document.structure.objects.get(&info_id) {
                if let Some(Object::String(date)) = info.get(b"ModDate") {
                    assert!(String::from_utf8_lossy(date).starts_with("D:"));
                }
            }
        }
    }
    
    #[test]
    fn test_cache_reset() {
        let mut cleaner = setup_test_cleaner();
        cleaner.cached_ids.insert(vec![1], vec![2]);
        cleaner.ref_counter = 42;
        
        cleaner.reset_cache();
        
        assert!(cleaner.cached_ids.is_empty());
        assert_eq!(cleaner.ref_counter, 0);
    }
}

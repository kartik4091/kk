//! Information dictionary cleaner for PDF anti-forensics
//! Created: 2025-06-03 14:58:31 UTC
//! Author: kartik4091

use std::collections::{HashMap, HashSet};
use chrono::{DateTime, Utc};
use regex::Regex;
use tracing::{debug, error, info, instrument, warn};

use crate::{
    error::{Error, Result},
    types::{Document, Object, ObjectId},
};

/// Handles PDF Information dictionary cleaning operations
#[derive(Debug)]
pub struct InfoCleaner {
    /// Cleaning statistics
    stats: CleaningStats,
    
    /// Fields to preserve
    preserve_fields: HashSet<String>,
    
    /// Custom field patterns to remove
    custom_patterns: Vec<Regex>,
    
    /// Replacement values for fields
    replacements: HashMap<String, String>,
}

/// PDF Information dictionary cleaning statistics
#[derive(Debug, Default)]
pub struct CleaningStats {
    /// Number of fields processed
    pub fields_processed: usize,
    
    /// Number of fields removed
    pub fields_removed: usize,
    
    /// Number of fields modified
    pub fields_modified: usize,
    
    /// Number of custom fields cleaned
    pub custom_fields_cleaned: usize,
    
    /// Processing duration in milliseconds
    pub duration_ms: u64,
}

/// Configuration for Info dictionary cleaning
#[derive(Debug, Clone)]
pub struct CleaningConfig {
    /// Fields to preserve (case-insensitive)
    pub preserve_fields: Vec<String>,
    
    /// Custom patterns to remove (regex)
    pub custom_patterns: Vec<String>,
    
    /// Replacement values for specific fields
    pub replacements: HashMap<String, String>,
    
    /// Remove all timestamps
    pub remove_timestamps: bool,
    
    /// Remove creation software info
    pub remove_software_info: bool,
    
    /// Remove all custom fields
    pub remove_custom_fields: bool,
}

/// Standard PDF Info dictionary fields
const STANDARD_FIELDS: [&str; 9] = [
    "Title",
    "Author",
    "Subject",
    "Keywords",
    "Creator",
    "Producer",
    "CreationDate",
    "ModDate",
    "Trapped",
];

impl Default for CleaningConfig {
    fn default() -> Self {
        Self {
            preserve_fields: Vec::new(),
            custom_patterns: Vec::new(),
            replacements: HashMap::new(),
            remove_timestamps: true,
            remove_software_info: true,
            remove_custom_fields: true,
        }
    }
}

impl InfoCleaner {
    /// Create a new InfoCleaner instance
    pub fn new() -> Result<Self> {
        Ok(Self {
            stats: CleaningStats::default(),
            preserve_fields: HashSet::new(),
            custom_patterns: Vec::new(),
            replacements: HashMap::new(),
        })
    }
    
    /// Configure the cleaner with provided settings
    #[instrument(skip(self, config))]
    pub fn configure(&mut self, config: &CleaningConfig) -> Result<()> {
        // Convert preserve fields to lowercase for case-insensitive comparison
        self.preserve_fields = config.preserve_fields
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
            
        // Compile regex patterns
        self.custom_patterns = config.custom_patterns
            .iter()
            .filter_map(|pattern| {
                match Regex::new(pattern) {
                    Ok(re) => Some(re),
                    Err(e) => {
                        warn!("Invalid regex pattern '{}': {}", pattern, e);
                        None
                    }
                }
            })
            .collect();
            
        // Store replacements
        self.replacements = config.replacements.clone();
        
        debug!("Cleaner configured with {} preserve fields and {} patterns",
            self.preserve_fields.len(), self.custom_patterns.len());
        Ok(())
    }
    
    /// Clean the Info dictionary in a document
    #[instrument(skip(self, document, config))]
    pub fn clean_info_dictionary(&mut self, document: &mut Document, config: &CleaningConfig) -> Result<()> {
        let start_time = std::time::Instant::now();
        info!("Starting Info dictionary cleaning");
        
        if let Some(info_id) = document.structure.trailer.info {
            if let Some(Object::Dictionary(info)) = document.structure.objects.get_mut(&info_id) {
                self.process_info_dictionary(info, config)?;
            } else {
                warn!("Info dictionary not found or invalid type");
            }
        } else {
            debug!("No Info dictionary present in document");
        }
        
        self.stats.duration_ms = start_time.elapsed().as_millis() as u64;
        info!("Info dictionary cleaning completed");
        Ok(())
    }
    
    /// Process and clean the Info dictionary
    fn process_info_dictionary(&mut self, info: &mut HashMap<Vec<u8>, Object>, config: &CleaningConfig) -> Result<()> {
        let mut keys_to_remove = Vec::new();
        let mut values_to_modify = HashMap::new();
        
        // First pass: identify fields to remove or modify
        for (key_bytes, value) in info.iter() {
            if let Ok(key) = String::from_utf8(key_bytes.clone()) {
                self.stats.fields_processed += 1;
                
                if self.should_remove_field(&key, value, config) {
                    keys_to_remove.push(key_bytes.clone());
                    self.stats.fields_removed += 1;
                } else if let Some(new_value) = self.get_replacement_value(&key, value, config) {
                    values_to_modify.insert(key_bytes.clone(), new_value);
                    self.stats.fields_modified += 1;
                }
            }
        }
        
        // Second pass: perform modifications
        for (key, value) in values_to_modify {
            info.insert(key, Object::String(value.into_bytes()));
        }
        
        // Third pass: remove fields
        for key in keys_to_remove {
            info.remove(&key);
        }
        
        Ok(())
    }
    
    /// Determine if a field should be removed
    fn should_remove_field(&self, key: &str, value: &Object, config: &CleaningConfig) -> bool {
        let key_lower = key.to_lowercase();
        
        // Check if field should be preserved
        if self.preserve_fields.contains(&key_lower) {
            return false;
        }
        
        // Handle standard fields
        if STANDARD_FIELDS.iter().any(|&f| f.eq_ignore_ascii_case(key)) {
            // Remove timestamps if configured
            if config.remove_timestamps && (key.ends_with("Date") || key.contains("Time")) {
                return true;
            }
            
            // Remove software info if configured
            if config.remove_software_info && (key == "Creator" || key == "Producer") {
                return true;
            }
            
            return false;
        }
        
        // Handle custom fields
        if config.remove_custom_fields {
            self.stats.custom_fields_cleaned += 1;
            return true;
        }
        
        // Check custom patterns
        if let Object::String(data) = value {
            if let Ok(text) = String::from_utf8(data.clone()) {
                return self.custom_patterns.iter().any(|re| re.is_match(&text));
            }
        }
        
        false
    }
    
    /// Get replacement value for a field
    fn get_replacement_value(&self, key: &str, value: &Object, config: &CleaningConfig) -> Option<String> {
        // Check explicit replacements
        if let Some(replacement) = self.replacements.get(key) {
            return Some(replacement.clone());
        }
        
        // Handle timestamps
        if (key.ends_with("Date") || key.contains("Time")) && !config.remove_timestamps {
            if let Object::String(data) = value {
                if let Ok(text) = String::from_utf8(data.clone()) {
                    return Some(self.sanitize_timestamp(&text));
                }
            }
        }
        
        None
    }
    
    /// Sanitize timestamp string
    fn sanitize_timestamp(&self, timestamp: &str) -> String {
        // Attempt to parse and reformat timestamp
        if let Ok(dt) = DateTime::parse_from_rfc3339(timestamp) {
            return dt.with_timezone(&Utc).format("%Y%m%d%H%M%S").to_string();
        }
        
        // Fallback: remove timezone and milliseconds
        let re = Regex::new(r"[\+\-]\d{2}:\d{2}|\.\d+").unwrap();
        re.replace_all(timestamp, "").to_string()
    }
    
    /// Get cleaning statistics
    pub fn statistics(&self) -> &CleaningStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn setup_test_cleaner() -> InfoCleaner {
        InfoCleaner::new().unwrap()
    }
    
    fn create_test_document() -> Document {
        let mut document = Document::default();
        let mut info_dict = HashMap::new();
        
        // Add standard fields
        info_dict.insert(b"Title".to_vec(), Object::String(b"Test Document".to_vec()));
        info_dict.insert(b"Author".to_vec(), Object::String(b"Test Author".to_vec()));
        info_dict.insert(b"CreationDate".to_vec(), Object::String(b"2025-06-03T14:58:31Z".to_vec()));
        info_dict.insert(b"Producer".to_vec(), Object::String(b"Test Software v1.0".to_vec()));
        
        // Add custom field
        info_dict.insert(b"CustomField".to_vec(), Object::String(b"Sensitive Data".to_vec()));
        
        let info_id = ObjectId { number: 1, generation: 0 };
        document.structure.objects.insert(info_id, Object::Dictionary(info_dict));
        document.structure.trailer.info = Some(info_id);
        
        document
    }
    
    #[test]
    fn test_cleaner_initialization() {
        let cleaner = setup_test_cleaner();
        assert!(cleaner.preserve_fields.is_empty());
        assert!(cleaner.custom_patterns.is_empty());
        assert!(cleaner.replacements.is_empty());
    }
    
    #[test]
    fn test_configuration() {
        let mut cleaner = setup_test_cleaner();
        let config = CleaningConfig {
            preserve_fields: vec!["Title".to_string()],
            custom_patterns: vec![r"Sensitive.*".to_string()],
            ..Default::default()
        };
        
        assert!(cleaner.configure(&config).is_ok());
        assert_eq!(cleaner.preserve_fields.len(), 1);
        assert_eq!(cleaner.custom_patterns.len(), 1);
    }
    
    #[test]
    fn test_clean_info_dictionary() {
        let mut cleaner = setup_test_cleaner();
        let mut document = create_test_document();
        
        let config = CleaningConfig {
            preserve_fields: vec!["Title".to_string()],
            remove_timestamps: true,
            remove_software_info: true,
            remove_custom_fields: true,
            ..Default::default()
        };
        
        cleaner.configure(&config).unwrap();
        cleaner.clean_info_dictionary(&mut document, &config).unwrap();
        
        if let Some(info_id) = document.structure.trailer.info {
            if let Some(Object::Dictionary(info)) = document.structure.objects.get(&info_id) {
                // Title should be preserved
                assert!(info.contains_key(b"Title"));
                
                // These should be removed
                assert!(!info.contains_key(b"CreationDate"));
                assert!(!info.contains_key(b"Producer"));
                assert!(!info.contains_key(b"CustomField"));
                
                // Check statistics
                let stats = cleaner.statistics();
                assert!(stats.fields_processed > 0);
                assert!(stats.fields_removed > 0);
                assert!(stats.custom_fields_cleaned > 0);
            }
        }
    }
    
    #[test]
    fn test_timestamp_sanitization() {
        let cleaner = setup_test_cleaner();
        
        // Test RFC3339 timestamp
        let sanitized = cleaner.sanitize_timestamp("2025-06-03T14:58:31Z");
        assert_eq!(sanitized, "20250603145831");
        
        // Test timestamp with timezone
        let sanitized = cleaner.sanitize_timestamp("2025-06-03T14:58:31+02:00");
        assert_eq!(sanitized, "20250603145831");
    }
    
    #[test]
    fn test_custom_pattern_removal() {
        let mut cleaner = setup_test_cleaner();
        let mut document = create_test_document();
        
        let config = CleaningConfig {
            custom_patterns: vec![r"Sensitive.*".to_string()],
            remove_custom_fields: false,
            ..Default::default()
        };
        
        cleaner.configure(&config).unwrap();
        cleaner.clean_info_dictionary(&mut document, &config).unwrap();
        
        if let Some(info_id) = document.structure.trailer.info {
            if let Some(Object::Dictionary(info)) = document.structure.objects.get(&info_id) {
                assert!(!info.contains_key(b"CustomField"));
            }
        }
    }
    
    #[test]
    fn test_field_replacement() {
        let mut cleaner = setup_test_cleaner();
        let mut document = create_test_document();
        
        let mut replacements = HashMap::new();
        replacements.insert("Author".to_string(), "Anonymous".to_string());
        
        let config = CleaningConfig {
            replacements,
            ..Default::default()
        };
        
        cleaner.configure(&config).unwrap();
        cleaner.clean_info_dictionary(&mut document, &config).unwrap();
        
        if let Some(info_id) = document.structure.trailer.info {
            if let Some(Object::Dictionary(info)) = document.structure.objects.get(&info_id) {
                if let Object::String(author) = &info[b"Author"] {
                    assert_eq!(String::from_utf8_lossy(author), "Anonymous");
                }
            }
        }
    }
          }

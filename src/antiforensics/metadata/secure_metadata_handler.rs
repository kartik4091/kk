//! Metadata handler implementation for PDF anti-forensics
//! Created: 2025-06-03 14:29:03 UTC
//! Author: kartik4091

use std::collections::HashMap;
use tracing::{debug, error, info, instrument, warn};

use crate::{
    error::{Error, Result},
    types::{Document, Object, ObjectId},
};

/// Handles metadata operations for PDF documents
pub struct MetadataHandler {
    /// Processing statistics
    stats: MetadataStats,
    
    /// Original metadata
    original: Option<Metadata>,
    
    /// Current metadata
    current: Option<Metadata>,
}

/// Metadata statistics
#[derive(Debug, Default)]
pub struct MetadataStats {
    /// Number of fields processed
    pub fields_processed: usize,
    
    /// Number of fields removed
    pub fields_removed: usize,
    
    /// Number of fields modified
    pub fields_modified: usize,
    
    /// Processing duration in milliseconds
    pub duration_ms: u64,
}

/// PDF metadata
#[derive(Debug, Clone)]
pub struct Metadata {
    /// Document title
    pub title: Option<String>,
    
    /// Document author
    pub author: Option<String>,
    
    /// Document subject
    pub subject: Option<String>,
    
    /// Document keywords
    pub keywords: Option<String>,
    
    /// Document creator
    pub creator: Option<String>,
    
    /// Document producer
    pub producer: Option<String>,
    
    /// Creation date
    pub creation_date: Option<String>,
    
    /// Modification date
    pub mod_date: Option<String>,
    
    /// Custom metadata fields
    pub custom: HashMap<String, String>,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            title: None,
            author: None,
            subject: None,
            keywords: None,
            creator: None,
            producer: None,
            creation_date: None,
            mod_date: None,
            custom: HashMap::new(),
        }
    }
}

/// Metadata cleaning configuration
#[derive(Debug, Clone)]
pub struct CleaningConfig {
    /// Remove all metadata
    pub remove_all: bool,
    
    /// Remove specific fields
    pub remove_fields: Vec<String>,
    
    /// Modify specific fields
    pub modify_fields: HashMap<String, String>,
    
    /// Remove timestamps
    pub remove_timestamps: bool,
    
    /// Remove custom fields
    pub remove_custom: bool,
}

impl Default for CleaningConfig {
    fn default() -> Self {
        Self {
            remove_all: false,
            remove_fields: Vec::new(),
            modify_fields: HashMap::new(),
            remove_timestamps: true,
            remove_custom: true,
        }
    }
}

impl MetadataHandler {
    /// Create a new metadata handler
    pub fn new() -> Self {
        Self {
            stats: MetadataStats::default(),
            original: None,
            current: None,
        }
    }
    
    /// Extract metadata from document
    #[instrument(skip(self, document))]
    pub fn extract_metadata(&mut self, document: &Document) -> Result<Metadata> {
        let start_time = std::time::Instant::now();
        info!("Extracting document metadata");
        
        let mut metadata = Metadata::default();
        
        // Extract from Info dictionary
        if let Some(info_id) = document.structure.trailer.info {
            if let Some(Object::Dictionary(info)) = document.structure.objects.get(&info_id) {
                self.extract_info_dict(&mut metadata, info);
            }
        }
        
        // Extract from XMP metadata
        if let Some(xmp_id) = self.find_xmp_metadata(document) {
            if let Some(Object::Stream { dict: _, data }) = document.structure.objects.get(&xmp_id) {
                self.extract_xmp_metadata(&mut metadata, data);
            }
        }
        
        self.stats.duration_ms += start_time.elapsed().as_millis() as u64;
        self.original = Some(metadata.clone());
        self.current = Some(metadata.clone());
        
        Ok(metadata)
    }
    
    /// Clean metadata according to configuration
    #[instrument(skip(self, document, config))]
    pub fn clean_metadata(&mut self, document: &mut Document, config: &CleaningConfig) -> Result<()> {
        let start_time = std::time::Instant::now();
        info!("Cleaning document metadata");
        
        if config.remove_all {
            self.remove_all_metadata(document)?;
            self.current = None;
        } else {
            let mut metadata = self.current.clone().unwrap_or_default();
            
            // Remove specified fields
            for field in &config.remove_fields {
                self.remove_field(&mut metadata, field);
            }
            
            // Modify specified fields
            for (field, value) in &config.modify_fields {
                self.modify_field(&mut metadata, field, value);
            }
            
            // Remove timestamps if configured
            if config.remove_timestamps {
                metadata.creation_date = None;
                metadata.mod_date = None;
            }
            
            // Remove custom fields if configured
            if config.remove_custom {
                metadata.custom.clear();
            }
            
            self.current = Some(metadata.clone());
            self.update_document_metadata(document, &metadata)?;
        }
        
        self.stats.duration_ms += start_time.elapsed().as_millis() as u64;
        Ok(())
    }
    
    /// Extract metadata from Info dictionary
    fn extract_info_dict(&mut self, metadata: &mut Metadata, info: &HashMap<Vec<u8>, Object>) {
        for (key, value) in info {
            if let Object::String(s) = value {
                let field_value = String::from_utf8_lossy(s).to_string();
                match key.as_slice() {
                    b"Title" => metadata.title = Some(field_value),
                    b"Author" => metadata.author = Some(field_value),
                    b"Subject" => metadata.subject = Some(field_value),
                    b"Keywords" => metadata.keywords = Some(field_value),
                    b"Creator" => metadata.creator = Some(field_value),
                    b"Producer" => metadata.producer = Some(field_value),
                    b"CreationDate" => metadata.creation_date = Some(field_value),
                    b"ModDate" => metadata.mod_date = Some(field_value),
                    _ => {
                        let key_str = String::from_utf8_lossy(key).to_string();
                        metadata.custom.insert(key_str, field_value);
                    }
                }
                self.stats.fields_processed += 1;
            }
        }
    }
    
    /// Extract metadata from XMP
    fn extract_xmp_metadata(&mut self, metadata: &mut Metadata, data: &[u8]) {
        if let Ok(xmp_str) = String::from_utf8(data.to_vec()) {
            // Basic XMP parsing
            if let Some(dc_title) = self.extract_xmp_value(&xmp_str, "dc:title") {
                metadata.title = Some(dc_title);
                self.stats.fields_processed += 1;
            }
            if let Some(dc_creator) = self.extract_xmp_value(&xmp_str, "dc:creator") {
                metadata.author = Some(dc_creator);
                self.stats.fields_processed += 1;
            }
            // Add more XMP field extractions as needed
        }
    }
    
    /// Find XMP metadata object
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
    
    /// Remove all metadata from document
    fn remove_all_metadata(&mut self, document: &mut Document) -> Result<()> {
        // Remove Info dictionary reference
        document.structure.trailer.info = None;
        
        // Remove XMP metadata
        if let Some(xmp_id) = self.find_xmp_metadata(document) {
            document.structure.objects.remove(&xmp_id);
        }
        
        self.stats.fields_removed += 1;
        Ok(())
    }
    
    /// Remove specific metadata field
    fn remove_field(&mut self, metadata: &mut Metadata, field: &str) {
        match field.to_lowercase().as_str() {
            "title" => {
                metadata.title = None;
                self.stats.fields_removed += 1;
            }
            "author" => {
                metadata.author = None;
                self.stats.fields_removed += 1;
            }
            "subject" => {
                metadata.subject = None;
                self.stats.fields_removed += 1;
            }
            "keywords" => {
                metadata.keywords = None;
                self.stats.fields_removed += 1;
            }
            "creator" => {
                metadata.creator = None;
                self.stats.fields_removed += 1;
            }
            "producer" => {
                metadata.producer = None;
                self.stats.fields_removed += 1;
            }
            _ => {
                if metadata.custom.remove(field).is_some() {
                    self.stats.fields_removed += 1;
                }
            }
        }
    }
    
    /// Modify metadata field
    fn modify_field(&mut self, metadata: &mut Metadata, field: &str, value: &str) {
        match field.to_lowercase().as_str() {
            "title" => {
                metadata.title = Some(value.to_string());
                self.stats.fields_modified += 1;
            }
            "author" => {
                metadata.author = Some(value.to_string());
                self.stats.fields_modified += 1;
            }
            "subject" => {
                metadata.subject = Some(value.to_string());
                self.stats.fields_modified += 1;
            }
            "keywords" => {
                metadata.keywords = Some(value.to_string());
                self.stats.fields_modified += 1;
            }
            "creator" => {
                metadata.creator = Some(value.to_string());
                self.stats.fields_modified += 1;
            }
            "producer" => {
                metadata.producer = Some(value.to_string());
                self.stats.fields_modified += 1;
            }
            _ => {
                metadata.custom.insert(field.to_string(), value.to_string());
                self.stats.fields_modified += 1;
            }
        }
    }
    
    /// Update document metadata
    fn update_document_metadata(&self, document: &mut Document, metadata: &Metadata) -> Result<()> {
        // Create new Info dictionary
        let mut info_dict = HashMap::new();
        
        if let Some(title) = &metadata.title {
            info_dict.insert(b"Title".to_vec(), Object::String(title.as_bytes().to_vec()));
        }
        if let Some(author) = &metadata.author {
            info_dict.insert(b"Author".to_vec(), Object::String(author.as_bytes().to_vec()));
        }
        if let Some(subject) = &metadata.subject {
            info_dict.insert(b"Subject".to_vec(), Object::String(subject.as_bytes().to_vec()));
        }
        if let Some(keywords) = &metadata.keywords {
            info_dict.insert(b"Keywords".to_vec(), Object::String(keywords.as_bytes().to_vec()));
        }
        if let Some(creator) = &metadata.creator {
            info_dict.insert(b"Creator".to_vec(), Object::String(creator.as_bytes().to_vec()));
        }
        if let Some(producer) = &metadata.producer {
            info_dict.insert(b"Producer".to_vec(), Object::String(producer.as_bytes().to_vec()));
        }
        
        for (key, value) in &metadata.custom {
            info_dict.insert(key.as_bytes().to_vec(), Object::String(value.as_bytes().to_vec()));
        }
        
        // Update document
        let info_id = ObjectId { number: document.structure.next_object_number(), generation: 0 };
        document.structure.objects.insert(info_id, Object::Dictionary(info_dict));
        document.structure.trailer.info = Some(info_id);
        
        Ok(())
    }
    
    /// Extract XMP value (basic implementation)
    fn extract_xmp_value(&self, xmp: &str, tag: &str) -> Option<String> {
        let start_tag = format!("<{}>", tag);
        let end_tag = format!("</{}>", tag);
        
        if let (Some(start), Some(end)) = (xmp.find(&start_tag), xmp.find(&end_tag)) {
            let value_start = start + start_tag.len();
            if value_start < end {
                return Some(xmp[value_start..end].trim().to_string());
            }
        }
        None
    }
    
    /// Get metadata statistics
    pub fn statistics(&self) -> &MetadataStats {
        &self.stats
    }
    
    /// Get original metadata
    pub fn original_metadata(&self) -> Option<&Metadata> {
        self.original.as_ref()
    }
    
    /// Get current metadata
    pub fn current_metadata(&self) -> Option<&Metadata> {
        self.current.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extract_metadata() {
        let mut handler = MetadataHandler::new();
        let mut document = Document::default();
        
        let mut info_dict = HashMap::new();
        info_dict.insert(b"Title".to_vec(), Object::String(b"Test Document".to_vec()));
        info_dict.insert(b"Author".to_vec(), Object::String(b"Test Author".to_vec()));
        
        let info_id = ObjectId { number: 1, generation: 0 };
        document.structure.objects.insert(info_id, Object::Dictionary(info_dict));
        document.structure.trailer.info = Some(info_id);
        
        let metadata = handler.extract_metadata(&document).unwrap();
        
        assert_eq!(metadata.title.unwrap(), "Test Document");
        assert_eq!(metadata.author.unwrap(), "Test Author");
        assert_eq!(handler.stats.fields_processed, 2);
    }
    
    #[test]
    fn test_clean_metadata() {
        let mut handler = MetadataHandler::new();
        let mut document = Document::default();
        
        let mut info_dict = HashMap::new();
        info_dict.insert(b"Title".to_vec(), Object::String(b"Test Document".to_vec()));
        info_dict.insert(b"Author".to_vec(), Object::String(b"Test Author".to_vec()));
        
        let info_id = ObjectId { number: 1, generation: 0 };
        document.structure.objects.insert(info_id, Object::Dictionary(info_dict));
        document.structure.trailer.info = Some(info_id);
        
        let config = CleaningConfig {
            remove_fields: vec!["Title".to_string()],
            ..Default::default()
        };
        
        handler.extract_metadata(&document).unwrap();
        handler.clean_metadata(&mut document, &config).unwrap();
        
        let cleaned = handler.current_metadata().unwrap();
        assert!(cleaned.title.is_none());
        assert!(cleaned.author.is_some());
    }
    
    #[test]
    fn test_modify_metadata() {
        let mut handler = MetadataHandler::new();
        let mut document = Document::default();
        
        let mut info_dict = HashMap::new();
        info_dict.insert(b"Title".to_vec(), Object::String(b"Test Document".to_vec()));
        
        let info_id = ObjectId { number: 1, generation: 0 };
        document.structure.objects.insert(info_id, Object::Dictionary(info_dict));
        document.structure.trailer.info = Some(info_id);
        
        let mut config = CleaningConfig::default();
        config.modify_fields.insert("Title".to_string(), "Modified Title".to_string());
        
        handler.extract_metadata(&document).unwrap();
        handler.clean_metadata(&mut document, &config).unwrap();
        
        let modified = handler.current_metadata().unwrap();
        assert_eq!(modified.title.unwrap(), "Modified Title");
        assert_eq!(handler.stats.fields_modified, 1);
    }
}

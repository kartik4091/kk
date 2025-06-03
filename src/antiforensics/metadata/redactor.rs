//! Metadata redaction implementation for PDF anti-forensics
//! Created: 2025-06-03 14:32:54 UTC
//! Author: kartik4091

use std::collections::{HashMap, HashSet};
use tracing::{debug, error, info, instrument, warn};

use crate::{
    error::{Error, Result},
    types::{Document, Object, ObjectId},
};

/// Handles metadata redaction in PDF documents
pub struct MetadataRedactor {
    /// Redaction statistics
    stats: RedactionStats,
    
    /// Sensitive patterns
    patterns: HashSet<String>,
    
    /// Replacement mapping
    replacements: HashMap<String, String>,
}

/// Redaction statistics
#[derive(Debug, Default)]
pub struct RedactionStats {
    /// Number of fields redacted
    pub fields_redacted: usize,
    
    /// Number of patterns matched
    pub patterns_matched: usize,
    
    /// Number of replacements made
    pub replacements_made: usize,
    
    /// Processing duration in milliseconds
    pub duration_ms: u64,
}

/// Redaction configuration
#[derive(Debug, Clone)]
pub struct RedactionConfig {
    /// Sensitive patterns to redact
    pub patterns: Vec<String>,
    
    /// Custom replacements
    pub replacements: HashMap<String, String>,
    
    /// Default replacement character
    pub default_char: char,
    
    /// Preserve length during redaction
    pub preserve_length: bool,
}

impl Default for RedactionConfig {
    fn default() -> Self {
        Self {
            patterns: Vec::new(),
            replacements: HashMap::new(),
            default_char: 'X',
            preserve_length: true,
        }
    }
}

impl MetadataRedactor {
    /// Create a new metadata redactor
    pub fn new() -> Self {
        Self {
            stats: RedactionStats::default(),
            patterns: HashSet::new(),
            replacements: HashMap::new(),
        }
    }
    
    /// Configure redactor
    pub fn configure(&mut self, config: &RedactionConfig) {
        self.patterns = config.patterns.iter().cloned().collect();
        self.replacements = config.replacements.clone();
    }
    
    /// Redact metadata in document
    #[instrument(skip(self, document, config))]
    pub fn redact_metadata(&mut self, document: &mut Document, config: &RedactionConfig) -> Result<()> {
        let start_time = std::time::Instant::now();
        info!("Starting metadata redaction");
        
        // Configure redactor with provided settings
        self.configure(config);
        
        // Redact Info dictionary
        if let Some(info_id) = document.structure.trailer.info {
            if let Some(Object::Dictionary(info)) = document.structure.objects.get_mut(&info_id) {
                self.redact_info_dictionary(info, config)?;
            }
        }
        
        // Redact XMP metadata
        if let Some(xmp_id) = self.find_xmp_metadata(document) {
            if let Some(Object::Stream { dict, data }) = document.structure.objects.get_mut(&xmp_id) {
                self.redact_xmp_metadata(data, config)?;
            }
        }
        
        self.stats.duration_ms = start_time.elapsed().as_millis() as u64;
        info!("Metadata redaction completed");
        Ok(())
    }
    
    /// Redact Info dictionary
    fn redact_info_dictionary(&mut self, info: &mut HashMap<Vec<u8>, Object>, config: &RedactionConfig) -> Result<()> {
        for (key, value) in info.iter_mut() {
            if let Object::String(text) = value {
                if let Ok(text_str) = String::from_utf8(text.clone()) {
                    if self.should_redact(&text_str) {
                        *text = self.redact_text(&text_str, config).into_bytes();
                        self.stats.fields_redacted += 1;
                    }
                }
            }
        }
        Ok(())
    }
    
    /// Redact XMP metadata
    fn redact_xmp_metadata(&mut self, data: &mut Vec<u8>, config: &RedactionConfig) -> Result<()> {
        if let Ok(mut xmp_str) = String::from_utf8(data.clone()) {
            let mut redacted = false;
            
            // Redact each XML element content
            for pattern in &self.patterns {
                if let Some(redacted_text) = self.redact_xml_content(&xmp_str, pattern, config) {
                    xmp_str = redacted_text;
                    redacted = true;
                }
            }
            
            if redacted {
                *data = xmp_str.into_bytes();
                self.stats.fields_redacted += 1;
            }
        }
        Ok(())
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
    
    /// Check if text should be redacted
    fn should_redact(&mut self, text: &str) -> bool {
        for pattern in &self.patterns {
            if text.contains(pattern) {
                self.stats.patterns_matched += 1;
                return true;
            }
        }
        false
    }
    
    /// Redact text content
    fn redact_text(&mut self, text: &str, config: &RedactionConfig) -> String {
        let mut redacted = text.to_string();
        
        for pattern in &self.patterns {
            if let Some(replacement) = self.replacements.get(pattern) {
                redacted = redacted.replace(pattern, replacement);
                self.stats.replacements_made += 1;
            } else if config.preserve_length {
                let replacement = config.default_char.to_string().repeat(pattern.len());
                redacted = redacted.replace(pattern, &replacement);
                self.stats.replacements_made += 1;
            } else {
                redacted = redacted.replace(pattern, &config.default_char.to_string());
                self.stats.replacements_made += 1;
            }
        }
        
        redacted
    }
    
    /// Redact XML content
    fn redact_xml_content(&mut self, xml: &str, pattern: &str, config: &RedactionConfig) -> Option<String> {
        let mut result = xml.to_string();
        let mut redacted = false;
        
        // Find content between XML tags
        let mut start = 0;
        while let Some(content_start) = result[start..].find('>') {
            if let Some(content_end) = result[start + content_start + 1..].find('<') {
                let content = &result[start + content_start + 1..start + content_start + 1 + content_end];
                if content.contains(pattern) {
                    let redacted_content = self.redact_text(content, config);
                    result.replace_range(
                        start + content_start + 1..start + content_start + 1 + content_end,
                        &redacted_content
                    );
                    redacted = true;
                }
                start = start + content_start + 1 + content_end;
            } else {
                break;
            }
        }
        
        if redacted {
            Some(result)
        } else {
            None
        }
    }
    
    /// Get redaction statistics
    pub fn statistics(&self) -> &RedactionStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_redact_text() {
        let mut redactor = MetadataRedactor::new();
        let config = RedactionConfig {
            patterns: vec!["confidential".to_string()],
            default_char: '*',
            preserve_length: true,
            ..Default::default()
        };
        
        redactor.configure(&config);
        let redacted = redactor.redact_text("This is confidential information", &config);
        
        assert!(!redacted.contains("confidential"));
        assert!(redacted.contains(&"*".repeat("confidential".len())));
    }
    
    #[test]
    fn test_redact_info_dictionary() {
        let mut redactor = MetadataRedactor::new();
        let config = RedactionConfig {
            patterns: vec!["secret".to_string()],
            default_char: 'X',
            preserve_length: true,
            ..Default::default()
        };
        
        let mut info = HashMap::new();
        info.insert(b"Title".to_vec(), Object::String(b"Top secret document".to_vec()));
        
        redactor.configure(&config);
        redactor.redact_info_dictionary(&mut info, &config).unwrap();
        
        if let Object::String(text) = &info[b"Title"] {
            assert!(!String::from_utf8_lossy(text).contains("secret"));
        }
    }
    
    #[test]
    fn test_redact_xml_content() {
        let mut redactor = MetadataRedactor::new();
        let config = RedactionConfig {
            patterns: vec!["sensitive".to_string()],
            default_char: '#',
            preserve_length: true,
            ..Default::default()
        };
        
        let xml = "<doc><title>This is sensitive data</title></doc>";
        
        redactor.configure(&config);
        let redacted = redactor.redact_xml_content(xml, "sensitive", &config).unwrap();
        
        assert!(!redacted.contains("sensitive"));
        assert!(redacted.contains(&"#".repeat("sensitive".len())));
    }
}

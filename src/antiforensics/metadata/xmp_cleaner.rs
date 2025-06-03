//! XMP metadata cleaner implementation for PDF anti-forensics
//! Created: 2025-06-03 15:00:35 UTC
//! Author: kartik4091

use std::collections::{HashMap, HashSet};
use quick_xml::{Reader, Writer, events::{Event, BytesStart, BytesEnd}};
use regex::Regex;
use tracing::{debug, error, info, instrument, warn};

use crate::{
    error::{Error, Result},
    types::{Document, Object, ObjectId},
};

/// Handles XMP metadata cleaning operations
#[derive(Debug)]
pub struct XMPCleaner {
    /// Cleaning statistics
    stats: CleaningStats,
    
    /// Namespaces to preserve
    preserve_ns: HashSet<String>,
    
    /// Elements to preserve
    preserve_elements: HashSet<String>,
    
    /// Custom patterns to remove
    patterns: Vec<Regex>,
    
    /// Replacement values
    replacements: HashMap<String, String>,
}

/// XMP cleaning statistics
#[derive(Debug, Default)]
pub struct CleaningStats {
    /// Number of elements processed
    pub elements_processed: usize,
    
    /// Number of elements removed
    pub elements_removed: usize,
    
    /// Number of attributes modified
    pub attributes_modified: usize,
    
    /// Number of text nodes cleaned
    pub text_nodes_cleaned: usize,
    
    /// Processing duration in milliseconds
    pub duration_ms: u64,
}

/// XMP cleaning configuration
#[derive(Debug, Clone)]
pub struct XMPConfig {
    /// Namespaces to preserve
    pub preserve_ns: Vec<String>,
    
    /// Elements to preserve
    pub preserve_elements: Vec<String>,
    
    /// Custom patterns to remove
    pub patterns: Vec<String>,
    
    /// Replacement values
    pub replacements: HashMap<String, String>,
    
    /// Remove all timestamps
    pub remove_timestamps: bool,
    
    /// Remove software information
    pub remove_software_info: bool,
    
    /// Remove document history
    pub remove_history: bool,
}

/// Standard XMP namespaces
const STANDARD_NS: [&str; 5] = [
    "http://ns.adobe.com/xap/1.0/",
    "http://purl.org/dc/elements/1.1/",
    "http://ns.adobe.com/pdf/1.3/",
    "http://ns.adobe.com/xap/1.0/mm/",
    "http://ns.adobe.com/pdfx/1.3/",
];

impl Default for XMPConfig {
    fn default() -> Self {
        Self {
            preserve_ns: Vec::new(),
            preserve_elements: Vec::new(),
            patterns: Vec::new(),
            replacements: HashMap::new(),
            remove_timestamps: true,
            remove_software_info: true,
            remove_history: true,
        }
    }
}

impl XMPCleaner {
    /// Create a new XMP cleaner instance
    pub fn new() -> Result<Self> {
        Ok(Self {
            stats: CleaningStats::default(),
            preserve_ns: HashSet::new(),
            preserve_elements: HashSet::new(),
            patterns: Vec::new(),
            replacements: HashMap::new(),
        })
    }
    
    /// Configure the XMP cleaner
    #[instrument(skip(self, config))]
    pub fn configure(&mut self, config: &XMPConfig) -> Result<()> {
        // Store namespaces to preserve
        self.preserve_ns = config.preserve_ns.iter().cloned().collect();
        
        // Store elements to preserve
        self.preserve_elements = config.preserve_elements.iter().cloned().collect();
        
        // Compile regex patterns
        self.patterns = config.patterns
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
        
        debug!("XMP cleaner configured with {} preserved namespaces and {} patterns",
            self.preserve_ns.len(), self.patterns.len());
        Ok(())
    }
    
    /// Clean XMP metadata in document
    #[instrument(skip(self, document, config))]
    pub fn clean_xmp(&mut self, document: &mut Document, config: &XMPConfig) -> Result<()> {
        let start_time = std::time::Instant::now();
        info!("Starting XMP metadata cleaning");
        
        if let Some(xmp_id) = self.find_xmp_metadata(document) {
            if let Some(Object::Stream { dict, data }) = document.structure.objects.get_mut(&xmp_id) {
                self.process_xmp_stream(data, config)?;
            } else {
                warn!("XMP metadata stream not found or invalid type");
            }
        } else {
            debug!("No XMP metadata present in document");
        }
        
        self.stats.duration_ms = start_time.elapsed().as_millis() as u64;
        info!("XMP metadata cleaning completed");
        Ok(())
    }
    
    /// Process XMP metadata stream
    fn process_xmp_stream(&mut self, data: &mut Vec<u8>, config: &XMPConfig) -> Result<()> {
        let mut reader = Reader::from_reader(data.as_slice());
        reader.trim_text(true);
        
        let mut writer = Writer::new(Vec::new());
        let mut buf = Vec::new();
        
        let mut in_metadata = false;
        let mut skip_element = false;
        let mut depth = 0;
        
        loop {
            match reader.read_event(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    depth += 1;
                    let name = std::str::from_utf8(e.name())?;
                    
                    if name == "xmpmeta" {
                        in_metadata = true;
                        writer.write_event(Event::Start(e.clone()))?;
                        continue;
                    }
                    
                    if !in_metadata {
                        writer.write_event(Event::Start(e.clone()))?;
                        continue;
                    }
                    
                    if self.should_skip_element(name, e, config) {
                        skip_element = true;
                        self.stats.elements_removed += 1;
                        continue;
                    }
                    
                    let mut elem = e.clone();
                    if let Some(modified) = self.process_attributes(elem, config) {
                        elem = modified;
                        self.stats.attributes_modified += 1;
                    }
                    
                    writer.write_event(Event::Start(elem))?;
                    self.stats.elements_processed += 1;
                },
                
                Ok(Event::End(ref e)) => {
                    depth -= 1;
                    
                    if skip_element {
                        if depth == 0 {
                            skip_element = false;
                        }
                        continue;
                    }
                    
                    writer.write_event(Event::End(e.clone()))?;
                    
                    if depth == 0 && in_metadata {
                        in_metadata = false;
                    }
                },
                
                Ok(Event::Text(ref e)) => {
                    if !skip_element {
                        let text = e.unescape_and_decode(&reader)?;
                        if let Some(cleaned) = self.clean_text(&text, config) {
                            writer.write_event(Event::Text(cleaned.into()))?;
                            self.stats.text_nodes_cleaned += 1;
                        } else {
                            writer.write_event(Event::Text(e.clone()))?;
                        }
                    }
                },
                
                Ok(Event::Eof) => break,
                
                Err(e) => {
                    error!("Error processing XMP: {}", e);
                    return Err(Error::XMPProcessingError(e.to_string()));
                },
                
                _ => {
                    if !skip_element {
                        writer.write_event(Event::Empty(BytesStart::owned_name("")))?;
                    }
                },
            }
            
            buf.clear();
        }
        
        *data = writer.into_inner();
        Ok(())
    }
    
    /// Determine if element should be skipped
    fn should_skip_element(&self, name: &str, element: &BytesStart, config: &XMPConfig) -> bool {
        // Check preserved elements
        if self.preserve_elements.contains(name) {
            return false;
        }
        
        // Check namespace
        if let Some(ns) = element.attributes()
            .find(|attr| attr.as_ref().map_or(false, |a| a.key == b"xmlns"))
            .and_then(|attr| attr.ok())
            .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok())
        {
            if self.preserve_ns.contains(&ns) {
                return false;
            }
        }
        
        // Check specific elements to remove
        if config.remove_timestamps && (name.contains("Date") || name.contains("Time")) {
            return true;
        }
        
        if config.remove_software_info && (name.contains("Creator") || name.contains("Producer")) {
            return true;
        }
        
        if config.remove_history && name.contains("History") {
            return true;
        }
        
        false
    }
    
    /// Process element attributes
    fn process_attributes(&self, mut element: BytesStart, config: &XMPConfig) 
        -> Option<BytesStart> {
        let mut modified = false;
        let mut new_attrs = Vec::new();
        
        for attr in element.attributes() {
            if let Ok(attr) = attr {
                let key = std::str::from_utf8(attr.key).unwrap_or("");
                let value = String::from_utf8_lossy(&attr.value);
                
                if let Some(new_value) = self.get_replacement_value(key, &value, config) {
                    new_attrs.push((attr.key.to_vec(), new_value.into_bytes()));
                    modified = true;
                } else {
                    new_attrs.push((attr.key.to_vec(), attr.value.to_vec()));
                }
            }
        }
        
        if modified {
            element.clear_attributes();
            for (key, value) in new_attrs {
                element.push_attribute((key.as_slice(), value.as_slice()));
            }
            Some(element)
        } else {
            None
        }
    }
    
    /// Clean text content
    fn clean_text(&self, text: &str, config: &XMPConfig) -> Option<String> {
        // Check patterns
        for pattern in &self.patterns {
            if pattern.is_match(text) {
                return Some(pattern.replace_all(text, "[REDACTED]").to_string());
            }
        }
        
        // Check replacements
        if let Some(replacement) = self.replacements.get(text) {
            return Some(replacement.clone());
        }
        
        None
    }
    
    /// Find XMP metadata in document
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
    
    /// Get replacement value
    fn get_replacement_value(&self, key: &str, value: &str, config: &XMPConfig) -> Option<String> {
        // Check explicit replacements
        if let Some(replacement) = self.replacements.get(value) {
            return Some(replacement.clone());
        }
        
        // Check patterns
        for pattern in &self.patterns {
            if pattern.is_match(value) {
                return Some(pattern.replace_all(value, "[REDACTED]").to_string());
            }
        }
        
        None
    }
    
    /// Get cleaning statistics
    pub fn statistics(&self) -> &CleaningStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn setup_test_cleaner() -> XMPCleaner {
        XMPCleaner::new().unwrap()
    }
    
    fn create_test_xmp() -> Vec<u8> {
        r#"<?xpacket begin="﻿" id="W5M0MpCehiHzreSzNTczkc9d"?>
        <x:xmpmeta xmlns:x="adobe:ns:meta/">
            <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
                <rdf:Description rdf:about=""
                    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
                    xmlns:dc="http://purl.org/dc/elements/1.1/"
                    xmlns:pdf="http://ns.adobe.com/pdf/1.3/">
                    <xmp:CreatorTool>Test Software</xmp:CreatorTool>
                    <xmp:CreateDate>2025-06-03T15:00:35Z</xmp:CreateDate>
                    <dc:title>Test Document</dc:title>
                    <pdf:Producer>Test Producer</pdf:Producer>
                </rdf:Description>
            </rdf:RDF>
        </x:xmpmeta>
        <?xpacket end="w"?>"#.as_bytes().to_vec()
    }
    
    #[test]
    fn test_cleaner_initialization() {
        let cleaner = setup_test_cleaner();
        assert!(cleaner.preserve_ns.is_empty());
        assert!(cleaner.preserve_elements.is_empty());
        assert!(cleaner.patterns.is_empty());
        assert!(cleaner.replacements.is_empty());
    }
    
    #[test]
    fn test_configuration() {
        let mut cleaner = setup_test_cleaner();
        let config = XMPConfig {
            preserve_ns: vec!["http://purl.org/dc/elements/1.1/".to_string()],
            preserve_elements: vec!["title".to_string()],
            patterns: vec![r"Test.*".to_string()],
            ..Default::default()
        };
        
        assert!(cleaner.configure(&config).is_ok());
        assert_eq!(cleaner.preserve_ns.len(), 1);
        assert_eq!(cleaner.preserve_elements.len(), 1);
        assert_eq!(cleaner.patterns.len(), 1);
    }
    
    #[test]
    fn test_xmp_cleaning() {
        let mut cleaner = setup_test_cleaner();
        let mut document = Document::default();
        let mut xmp_data = create_test_xmp();
        
        let xmp_id = ObjectId { number: 1, generation: 0 };
        let mut dict = HashMap::new();
        dict.insert(b"Type".to_vec(), Object::Name(b"Metadata".to_vec()));
        dict.insert(b"Subtype".to_vec(), Object::Name(b"XML".to_vec()));
        
        document.structure.objects.insert(xmp_id, Object::Stream {
            dict,
            data: xmp_data.clone(),
        });
        
        let config = XMPConfig {
            remove_timestamps: true,
            remove_software_info: true,
            ..Default::default()
        };
        
        cleaner.configure(&config).unwrap();
        cleaner.clean_xmp(&mut document, &config).unwrap();
        
        let stats = cleaner.statistics();
        assert!(stats.elements_processed > 0);
        assert!(stats.elements_removed > 0);
    }
    
    #[test]
    fn test_attribute_processing() {
        let cleaner = setup_test_cleaner();
        let mut element = BytesStart::owned_name("test");
        element.push_attribute(("key", "sensitive_value"));
        
        let mut replacements = HashMap::new();
        replacements.insert("sensitive_value".to_string(), "redacted".to_string());
        
        let config = XMPConfig {
            replacements,
            ..Default::default()
        };
        
        if let Some(modified) = cleaner.process_attributes(element, &config) {
            let attrs: Vec<_> = modified.attributes().collect();
            assert_eq!(attrs.len(), 1);
            assert_eq!(
                String::from_utf8_lossy(&attrs[0].as_ref().unwrap().value),
                "redacted"
            );
        }
    }
    
    #[test]
    fn test_text_cleaning() {
        let cleaner = setup_test_cleaner();
        let config = XMPConfig {
            patterns: vec![r"sensitive.*".to_string()],
            ..Default::default()
        };
        
        let text = "This is sensitive information";
        if let Some(cleaned) = cleaner.clean_text(text, &config) {
            assert!(cleaned.contains("[REDACTED]"));
            assert!(!cleaned.contains("sensitive"));
        }
    }
}

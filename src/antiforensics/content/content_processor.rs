//! Content stream processor implementation for PDF anti-forensics
//! Created: 2025-06-03 15:20:23 UTC
//! Author: kartik4091

use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use flate2::read::ZlibDecoder;
use lopdf::content::{Content, Operation};
use tracing::{debug, error, info, instrument, warn};

use crate::{
    error::{Error, Result},
    types::{Document, Object, ObjectId, Stream},
};

/// Handles PDF content stream processing operations
#[derive(Debug)]
pub struct ContentProcessor {
    /// Processing statistics
    stats: ProcessingStats,
    
    /// Operation cache for optimization
    op_cache: HashMap<Vec<u8>, Vec<Operation>>,
    
    /// Font references
    font_refs: HashSet<String>,
    
    /// Image references
    image_refs: HashSet<String>,
}

/// Content processing statistics
#[derive(Debug, Default)]
pub struct ProcessingStats {
    /// Number of content streams processed
    pub streams_processed: usize,
    
    /// Number of operations modified
    pub operations_modified: usize,
    
    /// Number of resources referenced
    pub resources_referenced: usize,
    
    /// Number of streams optimized
    pub streams_optimized: usize,
    
    /// Processing duration in milliseconds
    pub duration_ms: u64,
}

/// Content processing configuration
#[derive(Debug, Clone)]
pub struct ProcessingConfig {
    /// Remove text positioning operations
    pub remove_text_positioning: bool,
    
    /// Remove unnecessary graphics state
    pub remove_redundant_gs: bool,
    
    /// Optimize content streams
    pub optimize_streams: bool,
    
    /// Track resource dependencies
    pub track_resources: bool,
    
    /// Remove metadata comments
    pub remove_comments: bool,
}

impl Default for ProcessingConfig {
    fn default() -> Self {
        Self {
            remove_text_positioning: true,
            remove_redundant_gs: true,
            optimize_streams: true,
            track_resources: true,
            remove_comments: true,
        }
    }
}

impl ContentProcessor {
    /// Create new content processor instance
    pub fn new() -> Result<Self> {
        Ok(Self {
            stats: ProcessingStats::default(),
            op_cache: HashMap::new(),
            font_refs: HashSet::new(),
            image_refs: HashSet::new(),
        })
    }
    
    /// Process content streams in document
    #[instrument(skip(self, document, config))]
    pub fn process_content(&mut self, document: &mut Document, config: &ProcessingConfig) -> Result<()> {
        let start_time = std::time::Instant::now();
        info!("Starting content stream processing");
        
        // Process all content streams
        for (id, object) in document.structure.objects.iter_mut() {
            match object {
                Object::Stream(stream) => {
                    if self.is_content_stream(stream) {
                        self.process_stream(stream, config)?;
                        self.stats.streams_processed += 1;
                    }
                }
                Object::Dictionary(dict) => {
                    if let Some(contents) = dict.get(b"Contents") {
                        self.process_page_contents(contents, document, config)?;
                    }
                }
                _ => continue,
            }
        }
        
        self.stats.duration_ms = start_time.elapsed().as_millis() as u64;
        info!("Content stream processing completed");
        Ok(())
    }
    
    /// Process individual content stream
    fn process_stream(&mut self, stream: &mut Stream, config: &ProcessingConfig) -> Result<()> {
        // Decompress stream if needed
        let data = if let Some(filter) = stream.dict.get(b"Filter") {
            match filter {
                Object::Name(name) if name == b"FlateDecode" => {
                    let mut decoder = ZlibDecoder::new(Cursor::new(&stream.data));
                    let mut decoded = Vec::new();
                    std::io::copy(&mut decoder, &mut decoded)?;
                    decoded
                }
                _ => stream.data.clone(),
            }
        } else {
            stream.data.clone()
        };
        
        // Parse content stream
        let mut operations = self.parse_content(&data)?;
        
        // Process operations
        self.process_operations(&mut operations, config)?;
        
        // Optimize if configured
        if config.optimize_streams {
            self.optimize_operations(&mut operations)?;
            self.stats.streams_optimized += 1;
        }
        
        // Reconstruct stream
        let mut new_data = Vec::new();
        self.write_operations(&operations, &mut new_data)?;
        
        // Update stream data
        stream.data = new_data;
        
        Ok(())
    }
    
    /// Process page contents
    fn process_page_contents(
        &mut self,
        contents: &Object,
        document: &mut Document,
        config: &ProcessingConfig,
    ) -> Result<()> {
        match contents {
            Object::Array(array) => {
                for item in array {
                    if let Object::Reference(id) = item {
                        if let Some(Object::Stream(stream)) = document.structure.objects.get_mut(id) {
                            if self.is_content_stream(stream) {
                                self.process_stream(stream, config)?;
                                self.stats.streams_processed += 1;
                            }
                        }
                    }
                }
            }
            Object::Reference(id) => {
                if let Some(Object::Stream(stream)) = document.structure.objects.get_mut(id) {
                    if self.is_content_stream(stream) {
                        self.process_stream(stream, config)?;
                        self.stats.streams_processed += 1;
                    }
                }
            }
            _ => warn!("Unexpected Contents type"),
        }
        Ok(())
    }
    
    /// Parse content stream data
    fn parse_content(&self, data: &[u8]) -> Result<Vec<Operation>> {
        // Cache check
        if let Some(cached) = self.op_cache.get(data) {
            return Ok(cached.clone());
        }
        
        let content = Content::decode(data)
            .map_err(|e| Error::ContentProcessingError(format!("Failed to decode content: {}", e)))?;
            
        Ok(content.operations)
    }
    
    /// Process content stream operations
    fn process_operations(&mut self, operations: &mut Vec<Operation>, config: &ProcessingConfig) -> Result<()> {
        let mut i = 0;
        while i < operations.len() {
            let op = &operations[i];
            
            // Track resources if configured
            if config.track_resources {
                self.track_operation_resources(op);
            }
            
            // Remove unnecessary operations based on configuration
            if self.should_remove_operation(op, config) {
                operations.remove(i);
                self.stats.operations_modified += 1;
                continue;
            }
            
            // Modify operation if needed
            if let Some(modified) = self.modify_operation(op, config)? {
                operations[i] = modified;
                self.stats.operations_modified += 1;
            }
            
            i += 1;
        }
        
        Ok(())
    }
    
    /// Optimize content stream operations
    fn optimize_operations(&self, operations: &mut Vec<Operation>) -> Result<()> {
        // Remove consecutive identical graphics state operations
        operations.dedup_by(|a, b| {
            matches!((a.operator.as_str(), b.operator.as_str()),
                ("q", "q") | ("Q", "Q") | ("cm", "cm") | ("gs", "gs"))
        });
        
        // Combine text operations where possible
        let mut i = 0;
        while i < operations.len().saturating_sub(1) {
            if operations[i].operator == "Tj" && operations[i + 1].operator == "Tj" {
                if let (Some(text1), Some(text2)) = (
                    operations[i].operands.first(),
                    operations[i + 1].operands.first(),
                ) {
                    if let (Object::String(t1), Object::String(t2)) = (text1, text2) {
                        let mut combined = t1.clone();
                        combined.extend_from_slice(t2);
                        operations[i].operands = vec![Object::String(combined)];
                        operations.remove(i + 1);
                        continue;
                    }
                }
            }
            i += 1;
        }
        
        Ok(())
    }
    
    /// Track resource dependencies
    fn track_operation_resources(&mut self, operation: &Operation) {
        match operation.operator.as_str() {
            "Tf" => {
                if let Some(Object::Name(font)) = operation.operands.first() {
                    self.font_refs.insert(String::from_utf8_lossy(font).to_string());
                    self.stats.resources_referenced += 1;
                }
            }
            "Do" => {
                if let Some(Object::Name(xobject)) = operation.operands.first() {
                    self.image_refs.insert(String::from_utf8_lossy(xobject).to_string());
                    self.stats.resources_referenced += 1;
                }
            }
            _ => {}
        }
    }
    
    /// Determine if operation should be removed
    fn should_remove_operation(&self, operation: &Operation, config: &ProcessingConfig) -> bool {
        match operation.operator.as_str() {
            // Remove text positioning if configured
            "Td" | "TD" | "T*" if config.remove_text_positioning => true,
            
            // Remove redundant graphics state if configured
            "q" | "Q" | "cm" if config.remove_redundant_gs => true,
            
            // Remove comments if configured
            "%" if config.remove_comments => true,
            
            _ => false,
        }
    }
    
    /// Modify operation if needed
    fn modify_operation(&self, operation: &Operation, config: &ProcessingConfig) -> Result<Option<Operation>> {
        // Implement operation modifications here
        Ok(None)
    }
    
    /// Write operations back to stream
    fn write_operations(&self, operations: &[Operation], output: &mut Vec<u8>) -> Result<()> {
        for op in operations {
            // Write operands
            for operand in &op.operands {
                match operand {
                    Object::Integer(n) => write!(output, "{} ", n)?,
                    Object::Real(n) => write!(output, "{:.2} ", n)?,
                    Object::Boolean(b) => write!(output, "{} ", b)?,
                    Object::Name(n) => write!(output, "/{} ", String::from_utf8_lossy(n))?,
                    Object::String(s) => write!(output, "({}) ", String::from_utf8_lossy(s))?,
                    Object::Array(arr) => {
                        write!(output, "[ ")?;
                        for item in arr {
                            match item {
                                Object::Integer(n) => write!(output, "{} ", n)?,
                                Object::Real(n) => write!(output, "{:.2} ", n)?,
                                _ => {}
                            }
                        }
                        write!(output, "] ")?;
                    }
                    _ => {}
                }
            }
            
            // Write operator
            writeln!(output, "{}", op.operator)?;
        }
        
        Ok(())
    }
    
    /// Check if stream is a content stream
    fn is_content_stream(&self, stream: &Stream) -> bool {
        if let Some(Object::Name(subtype)) = stream.dict.get(b"Subtype") {
            return subtype == b"Form" || subtype == b"Image";
        }
        
        // Check for content stream operators
        if let Ok(operations) = self.parse_content(&stream.data) {
            return operations.iter().any(|op| {
                matches!(op.operator.as_str(),
                    "BT" | "ET" | "m" | "l" | "c" | "v" | "y" | "h" |
                    "re" | "S" | "s" | "f" | "F" | "f*" | "B" | "B*" |
                    "b" | "b*" | "n" | "W" | "W*")
            });
        }
        
        false
    }
    
    /// Get processing statistics
    pub fn statistics(&self) -> &ProcessingStats {
        &self.stats
    }
    
    /// Get font references
    pub fn font_references(&self) -> &HashSet<String> {
        &self.font_refs
    }
    
    /// Get image references
    pub fn image_references(&self) -> &HashSet<String> {
        &self.image_refs
    }
    
    /// Reset processor state
    pub fn reset(&mut self) {
        self.stats = ProcessingStats::default();
        self.op_cache.clear();
        self.font_refs.clear();
        self.image_refs.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn setup_test_processor() -> ContentProcessor {
        ContentProcessor::new().unwrap()
    }
    
    fn create_test_stream() -> Stream {
        Stream {
            dict: {
                let mut dict = HashMap::new();
                dict.insert(b"Length".to_vec(), Object::Integer(0));
                dict
            },
            data: b"BT /F1 12 Tf (Test) Tj ET".to_vec(),
        }
    }
    
    #[test]
    fn test_processor_initialization() {
        let processor = setup_test_processor();
        assert_eq!(processor.stats.streams_processed, 0);
        assert!(processor.op_cache.is_empty());
    }
    
    #[test]
    fn test_content_stream_detection() {
        let processor = setup_test_processor();
        let stream = create_test_stream();
        
        assert!(processor.is_content_stream(&stream));
    }
    
    #[test]
    fn test_operation_processing() {
        let mut processor = setup_test_processor();
        let config = ProcessingConfig::default();
        
        let mut operations = vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), Object::Integer(12)]),
            Operation::new("Tj", vec![Object::String(b"Test".to_vec())]),
            Operation::new("ET", vec![]),
        ];
        
        assert!(processor.process_operations(&mut operations, &config).is_ok());
    }
    
    #[test]
    fn test_resource_tracking() {
        let mut processor = setup_test_processor();
        
        let operation = Operation::new("Tf", vec![
            Object::Name(b"F1".to_vec()),
            Object::Integer(12),
        ]);
        
        processor.track_operation_resources(&operation);
        assert!(processor.font_references().contains("F1"));
    }
    
    #[test]
    fn test_stream_optimization() {
        let mut processor = setup_test_processor();
        let mut operations = vec![
            Operation::new("q", vec![]),
            Operation::new("q", vec![]),
            Operation::new("Q", vec![]),
        ];
        
        assert!(processor.optimize_operations(&mut operations).is_ok());
        assert_eq!(operations.len(), 2);
    }
    
    #[test]
    fn test_processor_reset() {
        let mut processor = setup_test_processor();
        
        // Add some data
        processor.stats.streams_processed = 1;
        processor.font_refs.insert("F1".to_string());
        processor.image_refs.insert("Im1".to_string());
        
        processor.reset();
        
        assert_eq!(processor.stats.streams_processed, 0);
        assert!(processor.font_refs.is_empty());
        assert!(processor.image_refs.is_empty());
    }
                 }

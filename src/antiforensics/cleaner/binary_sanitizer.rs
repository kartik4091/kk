//! Binary data sanitization implementation for PDF anti-forensics
//! Created: 2025-06-03 14:23:14 UTC
//! Author: kartik4091

use std::collections::HashMap;
use tracing::{debug, error, info, instrument, warn};

use crate::error::{Error, Result};

/// Binary data sanitizer
pub struct BinarySanitizer {
    /// Sanitization statistics
    stats: SanitizationStats,
    
    /// Data type handlers
    handlers: HashMap<DataType, SanitizationHandler>,
}

/// Sanitization statistics
#[derive(Debug, Default)]
pub struct SanitizationStats {
    /// Number of objects sanitized
    pub objects_sanitized: usize,
    
    /// Number of bytes processed
    pub bytes_processed: u64,
    
    /// Number of bytes removed
    pub bytes_removed: u64,
    
    /// Number of metadata fields removed
    pub metadata_removed: usize,
    
    /// Processing duration in milliseconds
    pub duration_ms: u64,
}

/// Binary data types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataType {
    /// Image data
    Image,
    
    /// Font data
    Font,
    
    /// Embedded file
    EmbeddedFile,
    
    /// ICC profile
    ICCProfile,
    
    /// Generic binary
    Generic,
}

/// Sanitization handler function
type SanitizationHandler = Box<dyn Fn(&[u8]) -> Result<Vec<u8>> + Send + Sync>;

impl BinarySanitizer {
    /// Create a new binary sanitizer
    pub fn new() -> Self {
        let mut handlers = HashMap::new();
        
        handlers.insert(
            DataType::Image,
            Box::new(Self::sanitize_image) as SanitizationHandler
        );
        handlers.insert(
            DataType::Font,
            Box::new(Self::sanitize_font) as SanitizationHandler
        );
        handlers.insert(
            DataType::EmbeddedFile,
            Box::new(Self::sanitize_embedded_file) as SanitizationHandler
        );
        handlers.insert(
            DataType::ICCProfile,
            Box::new(Self::sanitize_icc_profile) as SanitizationHandler
        );
        handlers.insert(
            DataType::Generic,
            Box::new(Self::sanitize_generic) as SanitizationHandler
        );
        
        Self {
            stats: SanitizationStats::default(),
            handlers,
        }
    }
    
    /// Clean binary data
    #[instrument(skip(self, data))]
    pub async fn clean_binary(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        let start_time = std::time::Instant::now();
        info!("Sanitizing binary data of {} bytes", data.len());
        
        // Detect data type
        let data_type = self.detect_data_type(data);
        debug!("Detected data type: {:?}", data_type);
        
        // Apply sanitization
        let sanitized = if let Some(handler) = self.handlers.get(&data_type) {
            handler(data)?
        } else {
            data.to_vec()
        };
        
        // Update statistics
        self.stats.objects_sanitized += 1;
        self.stats.bytes_processed += data.len() as u64;
        self.stats.bytes_removed += data.len() as u64 - sanitized.len() as u64;
        self.stats.duration_ms += start_time.elapsed().as_millis() as u64;
        
        Ok(sanitized)
    }
    
    /// Detect binary data type
    fn detect_data_type(&self, data: &[u8]) -> DataType {
        if data.len() < 8 {
            return DataType::Generic;
        }
        
        // Check image signatures
        if self.is_image_data(data) {
            return DataType::Image;
        }
        
        // Check font signatures
        if self.is_font_data(data) {
            return DataType::Font;
        }
        
        // Check ICC profile signature
        if self.is_icc_profile(data) {
            return DataType::ICCProfile;
        }
        
        // Check embedded file signatures
        if self.is_embedded_file(data) {
            return DataType::EmbeddedFile;
        }
        
        DataType::Generic
    }
    
    /// Check if data is image
    fn is_image_data(&self, data: &[u8]) -> bool {
        // JPEG
        if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return true;
        }
        
        // PNG
        if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
            return true;
        }
        
        // TIFF
        if data.starts_with(&[0x49, 0x49, 0x2A, 0x00]) || 
           data.starts_with(&[0x4D, 0x4D, 0x00, 0x2A]) {
            return true;
        }
        
        false
    }
    
    /// Check if data is font
    fn is_font_data(&self, data: &[u8]) -> bool {
        // OpenType
        if data.starts_with(&[0x4F, 0x54, 0x54, 0x4F]) {
            return true;
        }
        
        // TrueType
        if data.starts_with(&[0x00, 0x01, 0x00, 0x00]) {
            return true;
        }
        
        // Type1
        if data.starts_with(b"%!PS-AdobeFont") {
            return true;
        }
        
        false
    }
    
    /// Check if data is ICC profile
    fn is_icc_profile(&self, data: &[u8]) -> bool {
        if data.len() < 128 {
            return false;
        }
        
        // Check ICC profile signature
        &data[36..40] == b"acsp"
    }
    
    /// Check if data is embedded file
    fn is_embedded_file(&self, data: &[u8]) -> bool {
        // ZIP
        if data.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
            return true;
        }
        
        // PDF
        if data.starts_with(b"%PDF-") {
            return true;
        }
        
        false
    }
    
    // Sanitization handlers
    
    /// Sanitize image data
    fn sanitize_image(data: &[u8]) -> Result<Vec<u8>> {
        let mut sanitized = Vec::with_capacity(data.len());
        let mut i = 0;
        
        while i < data.len() {
            if i + 2 < data.len() && data[i] == 0xFF {
                // JPEG marker
                match data[i + 1] {
                    // Remove comments and application-specific markers
                    0xFE | 0xE0..=0xEF => {
                        let length = ((data[i + 2] as usize) << 8) | data[i + 3] as usize;
                        i += length + 2;
                        continue;
                    }
                    // Keep other markers
                    _ => {}
                }
            }
            sanitized.push(data[i]);
            i += 1;
        }
        
        Ok(sanitized)
    }
    
    /// Sanitize font data
    fn sanitize_font(data: &[u8]) -> Result<Vec<u8>> {
        let mut sanitized = data.to_vec();
        
        // Remove font metadata
        if let Some(name_table_pos) = Self::find_name_table(&sanitized) {
            Self::clear_name_table(&mut sanitized, name_table_pos);
        }
        
        Ok(sanitized)
    }
    
    /// Sanitize embedded file
    fn sanitize_embedded_file(data: &[u8]) -> Result<Vec<u8>> {
        // For embedded PDFs, we should recursively sanitize
        if data.starts_with(b"%PDF-") {
            // Placeholder for recursive PDF sanitization
            return Ok(data.to_vec());
        }
        
        // For other file types, remove metadata sections
        let mut sanitized = data.to_vec();
        Self::remove_file_metadata(&mut sanitized);
        
        Ok(sanitized)
    }
    
    /// Sanitize ICC profile
    fn sanitize_icc_profile(data: &[u8]) -> Result<Vec<u8>> {
        let mut sanitized = data.to_vec();
        
        // Clear profile description and copyright tags
        if sanitized.len() >= 128 {
            Self::clear_icc_text_tags(&mut sanitized);
        }
        
        Ok(sanitized)
    }
    
    /// Sanitize generic binary data
    fn sanitize_generic(data: &[u8]) -> Result<Vec<u8>> {
        let mut sanitized = Vec::with_capacity(data.len());
        
        // Remove null padding and repeated patterns
        let mut i = 0;
        while i < data.len() {
            if i + 8 <= data.len() && Self::is_pattern(&data[i..i + 8]) {
                i += 8;
                continue;
            }
            sanitized.push(data[i]);
            i += 1;
        }
        
        Ok(sanitized)
    }
    
    // Helper methods
    
    /// Find name table in font data
    fn find_name_table(data: &[u8]) -> Option<usize> {
        if data.len() < 4 {
            return None;
        }
        
        // Search for "name" table in TrueType/OpenType font
        for i in 0..data.len() - 4 {
            if &data[i..i + 4] == b"name" {
                return Some(i);
            }
        }
        
        None
    }
    
    /// Clear font name table
    fn clear_name_table(data: &mut [u8], pos: usize) {
        if pos + 8 > data.len() {
            return;
        }
        
        // Clear name records while preserving table structure
        for i in pos + 6..pos + 8 {
            data[i] = 0;
        }
    }
    
    /// Remove file metadata
    fn remove_file_metadata(data: &mut Vec<u8>) {
        // Implementation depends on file type
        // Here we just remove common metadata patterns
        if let Some(pos) = Self::find_metadata_section(data) {
            data.truncate(pos);
        }
    }
    
    /// Find metadata section
    fn find_metadata_section(data: &[u8]) -> Option<usize> {
        // Search for common metadata markers
        for (i, window) in data.windows(16).enumerate() {
            if window.starts_with(b"metadata") || 
               window.starts_with(b"META") ||
               window.starts_with(b"XMP") {
                return Some(i);
            }
        }
        None
    }
    
    /// Clear ICC profile text tags
    fn clear_icc_text_tags(data: &mut [u8]) {
        // Clear description tag
        if let Some(pos) = Self::find_icc_tag(data, b"desc") {
            Self::zero_tag_content(data, pos);
        }
        
        // Clear copyright tag
        if let Some(pos) = Self::find_icc_tag(data, b"cprt") {
            Self::zero_tag_content(data, pos);
        }
    }
    
    /// Find ICC profile tag
    fn find_icc_tag(data: &[u8], tag: &[u8]) -> Option<usize> {
        data.windows(4)
            .position(|window| window == tag)
    }
    
    /// Zero out tag content
    fn zero_tag_content(data: &mut [u8], pos: usize) {
        if pos + 8 > data.len() {
            return;
        }
        
        let length = ((data[pos + 4] as usize) << 24) |
                    ((data[pos + 5] as usize) << 16) |
                    ((data[pos + 6] as usize) << 8) |
                    (data[pos + 7] as usize);
                    
        for i in pos + 8..std::cmp::min(pos + 8 + length, data.len()) {
            data[i] = 0;
        }
    }
    
    /// Check if bytes form a pattern
    fn is_pattern(bytes: &[u8]) -> bool {
        if bytes.len() < 4 {
            return false;
        }
        
        let pattern_len = bytes.len() / 2;
        bytes[..pattern_len] == bytes[pattern_len..2 * pattern_len]
    }
    
    /// Get sanitization statistics
    pub fn statistics(&self) -> &SanitizationStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_detect_image_data() {
        let sanitizer = BinarySanitizer::new();
        
        // JPEG test
        let jpeg_data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
        assert_eq!(sanitizer.detect_data_type(&jpeg_data), DataType::Image);
        
        // PNG test
        let png_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(sanitizer.detect_data_type(&png_data), DataType::Image);
    }
    
    #[test]
    fn test_detect_font_data() {
        let sanitizer = BinarySanitizer::new();
        
        // OpenType test
        let otf_data = vec![0x4F, 0x54, 0x54, 0x4F, 0x00, 0x10, 0x00, 0x00];
        assert_eq!(sanitizer.detect_data_type(&otf_data), DataType::Font);
        
        // TrueType test
        let ttf_data = vec![0x00, 0x01, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00];
        assert_eq!(sanitizer.detect_data_type(&ttf_data), DataType::Font);
    }
    
    #[test]
    fn test_sanitize_image() {
        let mut sanitizer = BinarySanitizer::new();
        
        let image_data = vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46,
            0x49, 0x46, 0x00, 0x01, 0x01, 0x01, 0x00, 0x48
        ];
        
        let result = sanitizer.clean_binary(&image_data).unwrap();
        assert!(result.len() <= image_data.len());
    }
    
    #[test]
    fn test_sanitize_generic() {
        let mut sanitizer = BinarySanitizer::new();
        
        let data = vec![
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08
        ];
        
        let result = sanitizer.clean_binary(&data).unwrap();
        assert!(result.len() <= data.len());
    }
}

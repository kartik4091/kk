//! Font//! Font processing implementation for PDF anti-forensics
//! Created: 2025-06-03 15:22:54 UTC
//! Author: kartik4091

use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use freetype::{Face, Library};
use ttf_parser::{Face as TTFace, GlyphId};
use tracing::{debug, error, info, instrument, warn};

use crate::{
    error::{Error, Result},
    types::{Document, Object, ObjectId, Stream},
};

/// Handles PDF font processing operations
#[derive(Debug)]
pub struct FontProcessor {
    /// Processing statistics
    stats: ProcessingStats,
    
    /// FreeType library instance
    ft_library: Library,
    
    /// Font cache for optimization
    font_cache: HashMap<Vec<u8>, Face>,
    
    /// Glyph usage tracking
    glyph_usage: HashMap<String, HashSet<GlyphId>>,
    
    /// Font subset mappings
    subset_mappings: HashMap<String, HashMap<GlyphId, GlyphId>>,
}

/// Font processing statistics
#[derive(Debug, Default)]
pub struct ProcessingStats {
    /// Number of fonts processed
    pub fonts_processed: usize,
    
    /// Number of fonts subsetted
    pub fonts_subsetted: usize,
    
    /// Number of glyphs removed
    pub glyphs_removed: usize,
    
    /// Number of bytes saved
    pub bytes_saved: u64,
    
    /// Processing duration in milliseconds
    pub duration_ms: u64,
}

/// Font processing configuration
#[derive(Debug, Clone)]
pub struct ProcessingConfig {
    /// Enable font subsetting
    pub enable_subsetting: bool,
    
    /// Remove unused glyphs
    pub remove_unused_glyphs: bool,
    
    /// Strip font metadata
    pub strip_metadata: bool,
    
    /// Minimum subsetting threshold (percentage)
    pub subset_threshold: f32,
    
    /// Preserve font names
    pub preserve_names: bool,
}

/// Font information structure
#[derive(Debug)]
pub struct FontInfo {
    /// Font name
    pub name: String,
    
    /// Font type
    pub font_type: FontType,
    
    /// Encoding
    pub encoding: String,
    
    /// Original size in bytes
    pub original_size: usize,
    
    /// Processed size in bytes
    pub processed_size: Option<usize>,
    
    /// Number of glyphs
    pub glyph_count: usize,
    
    /// Used glyphs
    pub used_glyphs: HashSet<GlyphId>,
}

/// Font types supported
#[derive(Debug, Clone, PartialEq)]
pub enum FontType {
    /// TrueType font
    TrueType,
    
    /// Type 1 font
    Type1,
    
    /// CID font
    CIDFont,
    
    /// OpenType font
    OpenType,
}

impl Default for ProcessingConfig {
    fn default() -> Self {
        Self {
            enable_subsetting: true,
            remove_unused_glyphs: true,
            strip_metadata: true,
            subset_threshold: 0.3, // 30% usage threshold
            preserve_names: false,
        }
    }
}

impl FontProcessor {
    /// Create new font processor instance
    pub fn new() -> Result<Self> {
        Ok(Self {
            stats: ProcessingStats::default(),
            ft_library: Library::init()?,
            font_cache: HashMap::new(),
            glyph_usage: HashMap::new(),
            subset_mappings: HashMap::new(),
        })
    }
    
    /// Process fonts in document
    #[instrument(skip(self, document, config))]
    pub fn process_fonts(&mut self, document: &mut Document, config: &ProcessingConfig) -> Result<()> {
        let start_time = std::time::Instant::now();
        info!("Starting font processing");
        
        // Collect font resources
        let font_objects = self.collect_font_objects(document)?;
        
        // Process each font
        for (font_id, font_dict) in font_objects {
            match self.process_font(font_id, font_dict, document, config) {
                Ok(_) => {
                    self.stats.fonts_processed += 1;
                    debug!("Processed font: {:?}", font_id);
                }
                Err(e) => {
                    error!("Failed to process font {:?}: {}", font_id, e);
                    continue;
                }
            }
        }
        
        self.stats.duration_ms = start_time.elapsed().as_millis() as u64;
        info!("Font processing completed");
        Ok(())
    }
    
    /// Collect font objects from document
    fn collect_font_objects(&self, document: &Document) -> Result<Vec<(ObjectId, &Object)>> {
        let mut fonts = Vec::new();
        
        // Search through all objects
        for (id, object) in &document.structure.objects {
            if let Object::Dictionary(dict) = object {
                if let Some(Object::Name(type_name)) = dict.get(b"Type") {
                    if type_name == b"Font" {
                        fonts.push((*id, object));
                    }
                }
            }
        }
        
        Ok(fonts)
    }
    
    /// Process individual font
    fn process_font(
        &mut self,
        font_id: ObjectId,
        font_obj: &Object,
        document: &mut Document,
        config: &ProcessingConfig,
    ) -> Result<()> {
        let font_dict = match font_obj {
            Object::Dictionary(dict) => dict,
            _ => return Err(Error::InvalidFontObject),
        };
        
        // Extract font information
        let font_info = self.extract_font_info(font_dict)?;
        
        // Track original size
        let original_size = self.calculate_font_size(font_dict);
        
        // Process based on font type
        match font_info.font_type {
            FontType::TrueType | FontType::OpenType => {
                self.process_ttf_font(font_id, font_dict, document, config)?;
            }
            FontType::Type1 => {
                self.process_type1_font(font_id, font_dict, document, config)?;
            }
            FontType::CIDFont => {
                self.process_cid_font(font_id, font_dict, document, config)?;
            }
        }
        
        // Calculate space saved
        if let Some(processed_size) = font_info.processed_size {
            self.stats.bytes_saved += (original_size - processed_size) as u64;
        }
        
        Ok(())
    }
    
    /// Process TrueType/OpenType font
    fn process_ttf_font(
        &mut self,
        font_id: ObjectId,
        font_dict: &HashMap<Vec<u8>, Object>,
        document: &mut Document,
        config: &ProcessingConfig,
    ) -> Result<()> {
        // Extract font data
        let font_data = self.extract_font_data(font_dict, document)?;
        
        // Parse font
        let face = TTFace::parse(&font_data, 0)
            .map_err(|e| Error::FontProcessingError(format!("Failed to parse TTF: {}", e)))?;
            
        if config.enable_subsetting {
            // Collect used glyphs
            let used_glyphs = self.collect_used_glyphs(&face, document)?;
            
            // Check subsetting threshold
            if (used_glyphs.len() as f32 / face.number_of_glyphs() as f32) < config.subset_threshold {
                // Create font subset
                let subset_data = self.create_font_subset(&face, &used_glyphs, config)?;
                
                // Update font in document
                self.update_font_data(font_id, subset_data, document)?;
                
                self.stats.fonts_subsetted += 1;
                self.stats.glyphs_removed += face.number_of_glyphs() - used_glyphs.len();
            }
        }
        
        if config.strip_metadata {
            self.strip_font_metadata(font_id, document)?;
        }
        
        Ok(())
    }
    
    /// Process Type 1 font
    fn process_type1_font(
        &mut self,
        font_id: ObjectId,
        font_dict: &HashMap<Vec<u8>, Object>,
        document: &mut Document,
        config: &ProcessingConfig,
    ) -> Result<()> {
        // Type 1 specific processing
        if config.strip_metadata {
            self.strip_type1_metadata(font_id, document)?;
        }
        
        Ok(())
    }
    
    /// Process CID font
    fn process_cid_font(
        &mut self,
        font_id: ObjectId,
        font_dict: &HashMap<Vec<u8>, Object>,
        document: &mut Document,
        config: &ProcessingConfig,
    ) -> Result<()> {
        // CID font specific processing
        if config.strip_metadata {
            self.strip_cid_metadata(font_id, document)?;
        }
        
        Ok(())
    }
    
    /// Extract font information
    fn extract_font_info(&self, font_dict: &HashMap<Vec<u8>, Object>) -> Result<FontInfo> {
        let name = self.extract_font_name(font_dict)?;
        let font_type = self.determine_font_type(font_dict)?;
        let encoding = self.extract_encoding(font_dict)?;
        let original_size = self.calculate_font_size(font_dict);
        
        Ok(FontInfo {
            name,
            font_type,
            encoding,
            original_size,
            processed_size: None,
            glyph_count: 0,
            used_glyphs: HashSet::new(),
        })
    }
    
    /// Extract font name
    fn extract_font_name(&self, font_dict: &HashMap<Vec<u8>, Object>) -> Result<String> {
        if let Some(Object::Name(name)) = font_dict.get(b"BaseFont") {
            Ok(String::from_utf8_lossy(name).to_string())
        } else {
            Ok("Unknown".to_string())
        }
    }
    
    /// Determine font type
    fn determine_font_type(&self, font_dict: &HashMap<Vec<u8>, Object>) -> Result<FontType> {
        if let Some(Object::Name(subtype)) = font_dict.get(b"Subtype") {
            match subtype.as_slice() {
                b"TrueType" => Ok(FontType::TrueType),
                b"Type1" => Ok(FontType::Type1),
                b"CIDFontType0" | b"CIDFontType2" => Ok(FontType::CIDFont),
                b"OpenType" => Ok(FontType::OpenType),
                _ => Err(Error::UnsupportedFontType),
            }
        } else {
            Err(Error::InvalidFontObject)
        }
    }
    
    /// Extract font encoding
    fn extract_encoding(&self, font_dict: &HashMap<Vec<u8>, Object>) -> Result<String> {
        if let Some(Object::Name(encoding)) = font_dict.get(b"Encoding") {
            Ok(String::from_utf8_lossy(encoding).to_string())
        } else {
            Ok("StandardEncoding".to_string())
        }
    }
    
    /// Calculate font size
    fn calculate_font_size(&self, font_dict: &HashMap<Vec<u8>, Object>) -> usize {
        let mut size = 0;
        
        // Add dictionary size
        for (key, value) in font_dict {
            size += key.len();
            size += match value {
                Object::String(s) | Object::Name(s) => s.len(),
                Object::Integer(_) | Object::Real(_) => 8,
                _ => 0,
            };
        }
        
        size
    }
    
    /// Get processing statistics
    pub fn statistics(&self) -> &ProcessingStats {
        &self.stats
    }
    
    /// Reset processor state
    pub fn reset(&mut self) {
        self.stats = ProcessingStats::default();
        self.font_cache.clear();
        self.glyph_usage.clear();
        self.subset_mappings.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn setup_test_processor() -> FontProcessor {
        FontProcessor::new().unwrap()
    }
    
    fn create_test_font_dict() -> HashMap<Vec<u8>, Object> {
        let mut dict = HashMap::new();
        dict.insert(b"Type".to_vec(), Object::Name(b"Font".to_vec()));
        dict.insert(b"Subtype".to_vec(), Object::Name(b"TrueType".to_vec()));
        dict.insert(b"BaseFont".to_vec(), Object::Name(b"TestFont".to_vec()));
        dict
    }
    
    #[test]
    fn test_processor_initialization() {
        let processor = setup_test_processor();
        assert_eq!(processor.stats.fonts_processed, 0);
        assert!(processor.font_cache.is_empty());
    }
    
    #[test]
    fn test_font_type_determination() {
        let processor = setup_test_processor();
        let font_dict = create_test_font_dict();
        
        let font_type = processor.determine_font_type(&font_dict).unwrap();
        assert_eq!(font_type, FontType::TrueType);
    }
    
    #[test]
    fn test_font_name_extraction() {
        let processor = setup_test_processor();
        let font_dict = create_test_font_dict();
        
        let font_name = processor.extract_font_name(&font_dict).unwrap();
        assert_eq!(font_name, "TestFont");
    }
    
    #[test]
    fn test_font_info_extraction() {
        let processor = setup_test_processor();
        let font_dict = create_test_font_dict();
        
        let font_info = processor.extract_font_info(&font_dict).unwrap();
        assert_eq!(font_info.name, "TestFont");
        assert_eq!(font_info.font_type, FontType::TrueType);
    }
    
    #[test]
    fn test_processor_reset() {
        let mut processor = setup_test_processor();
        
        // Add some data
        processor.stats.fonts_processed = 1;
        processor.glyph_usage.insert("TestFont".to_string(), HashSet::new());
        
        processor.reset();
        
        assert_eq!(processor.stats.fonts_processed, 0);
        assert!(processor.glyph_usage.is_empty());
    }
} processing module
//! Created: 2025-06-03 12:50:15 UTC
//! Author: kartik4091


// Auto-generated for kartik4091/kk
// Timestamp: 2025-06-02 05:43:33
// User: kartik4091

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::{debug, error, info, instrument, warn};
use crate::core::error::PdfError;

#[derive(Debug, thiserror::Error)]
pub enum FontError {
    #[error("Font loading error: {0}")]
    LoadError(String),
    
    #[error("Font rendering error: {0}")]
    RenderError(String),
    
    #[error("Font configuration error: {0}")]
    ConfigError(String),
    
    #[error("Font validation error: {0}")]
    ValidationError(String),
    
    #[error(transparent)]
    Storage(#[from] std::io::Error),
    
    #[error(transparent)]
    Core(#[from] PdfError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontConfig {
    pub fonts: HashMap<String, FontInfo>,
    pub fallbacks: Vec<String>,
    pub rendering: RenderingConfig,
    pub cache: CacheConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontInfo {
    pub name: String,
    pub family: String,
    pub style: FontStyle,
    pub weight: FontWeight,
    pub path: String,
    pub format: FontFormat,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FontWeight {
    Thin,
    ExtraLight,
    Light,
    Regular,
    Medium,
    SemiBold,
    Bold,
    ExtraBold,
    Black,
    Custom(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FontFormat {
    TTF,
    OTF,
    WOFF,
    WOFF2,
    Type1,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderingConfig {
    pub antialiasing: bool,
    pub hinting: HintingMode,
    pub subpixel: SubpixelMode,
    pub lcd_filter: LcdFilterMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HintingMode {
    None,
    Slight,
    Medium,
    Full,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SubpixelMode {
    None,
    RGB,
    BGR,
    VRGB,
    VBGR,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LcdFilterMode {
    None,
    Default,
    Light,
    Legacy,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub enabled: bool,
    pub max_size_mb: u32,
    pub ttl_seconds: u32,
    pub persistent: bool,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            fonts: HashMap::new(),
            fallbacks: vec!["Arial".to_string(), "Helvetica".to_string()],
            rendering: RenderingConfig {
                antialiasing: true,
                hinting: HintingMode::Medium,
                subpixel: SubpixelMode::RGB,
                lcd_filter: LcdFilterMode::Default,
            },
            cache: CacheConfig {
                enabled: true,
                max_size_mb: 100,
                ttl_seconds: 3600,
                persistent: false,
            },
        }
    }
}

#[derive(Debug)]
pub struct FontManager {
    config: FontConfig,
    state: Arc<RwLock<FontState>>,
    metrics: Arc<FontMetrics>,
}

#[derive(Debug, Default)]
struct FontState {
    loaded_fonts: HashMap<String, LoadedFont>,
    cache: FontCache,
    glyph_cache: GlyphCache,
}

#[derive(Debug)]
struct LoadedFont {
    info: FontInfo,
    data: Vec<u8>,
    metrics: FontMetrics,
    last_used: DateTime<Utc>,
    usage_count: u64,
}

#[derive(Debug, Clone)]
pub struct FontMetrics {
    ascent: f32,
    descent: f32,
    line_height: f32,
    units_per_em: u32,
    x_height: f32,
    cap_height: f32,
}

#[derive(Debug, Default)]
struct FontCache {
    entries: HashMap<String, CacheEntry>,
    size: u64,
}

#[derive(Debug)]
struct CacheEntry {
    data: Vec<u8>,
    size: u64,
    last_accessed: DateTime<Utc>,
    hits: u64,
}

#[derive(Debug, Default)]
struct GlyphCache {
    entries: HashMap<GlyphKey, GlyphData>,
    size: u64,
}

#[derive(Debug, Hash, Eq, PartialEq)]
struct GlyphKey {
    font_id: String,
    char_code: char,
    size: u32,
}

#[derive(Debug, Clone)]
struct GlyphData {
    bitmap: Vec<u8>,
    metrics: GlyphMetrics,
    size: u64,
}

#[derive(Debug, Clone)]
struct GlyphMetrics {
    width: f32,
    height: f32,
    bearing_x: f32,
    bearing_y: f32,
    advance: f32,
}

#[derive(Debug)]
struct FontMetrics {
    loaded_fonts: prometheus::Gauge,
    cache_hits: prometheus::IntCounter,
    cache_misses: prometheus::IntCounter,
    render_time: prometheus::Histogram,
}

#[async_trait]
pub trait FontLoader {
    async fn load_font(&mut self, info: FontInfo) -> Result<String, FontError>;
    async fn unload_font(&mut self, font_id: &str) -> Result<(), FontError>;
    async fn get_font_metrics(&self, font_id: &str) -> Result<FontMetrics, FontError>;
    async fn get_font_info(&self, font_id: &str) -> Result<FontInfo, FontError>;
}

#[async_trait]
pub trait GlyphRenderer {
    async fn render_glyph(&mut self, font_id: &str, character: char, size: u32) -> Result<Vec<u8>, FontError>;
    async fn get_glyph_metrics(&self, font_id: &str, character: char, size: u32) -> Result<GlyphMetrics, FontError>;
    async fn cache_glyph(&mut self, font_id: &str, character: char, size: u32, data: Vec<u8>) -> Result<(), FontError>;
}

impl FontManager {
    pub fn new(config: FontConfig) -> Self {
        let metrics = Arc::new(FontMetrics::new());
        
        Self {
            config,
            state: Arc::new(RwLock::new(FontState::default())),
            metrics,
        }
    }

    #[instrument(skip(self))]
    pub async fn initialize(&self) -> Result<(), FontError> {
        info!("Initializing FontManager");
        self.validate_config().await?;
        self.preload_fonts().await?;
        Ok(())
    }

    async fn validate_config(&self) -> Result<(), FontError> {
        for (font_id, font_info) in &self.config.fonts {
            if font_info.path.is_empty() {
                return Err(FontError::ConfigError(
                    format!("Invalid font path for font: {}", font_id)
                ));
            }

            match font_info.format {
                FontFormat::TTF | FontFormat::OTF | FontFormat::WOFF | FontFormat::WOFF2 => {},
                FontFormat::Type1 => {
                    warn!("Type1 fonts are deprecated");
                },
                FontFormat::Custom(ref format) => {
                    debug!("Using custom font format: {}", format);
                },
            }
        }

        if self.config.cache.max_size_mb == 0 {
            return Err(FontError::ConfigError("Invalid cache size".to_string()));
        }

        Ok(())
    }

    async fn preload_fonts(&self) -> Result<(), FontError> {
        let mut state = self.state.write().await;
        
        for (font_id, font_info) in &self.config.fonts {
            if let Ok(data) = tokio::fs::read(&font_info.path).await {
                let metrics = self.compute_font_metrics(&data)?;
                
                state.loaded_fonts.insert(font_id.clone(), LoadedFont {
                    info: font_info.clone(),
                    data,
                    metrics: metrics.clone(),
                    last_used: Utc::now(),
                    usage_count: 0,
                });
                
                self.metrics.loaded_fonts.inc();
            } else {
                warn!("Failed to preload font: {}", font_id);
            }
        }
        
        Ok(())
    }

    fn compute_font_metrics(&self, font_data: &[u8]) -> Result<FontMetrics, FontError> {
        // In a real implementation, this would compute actual font metrics
        Ok(FontMetrics {
            ascent: 0.0,
            descent: 0.0,
            line_height: 0.0,
            units_per_em: 0,
            x_height: 0.0,
            cap_height: 0.0,
        })
    }

    async fn render_glyph_internal(&self, font: &LoadedFont, character: char, size: u32) -> Result<GlyphData, FontError> {
        // In a real implementation, this would render the actual glyph
        Ok(GlyphData {
            bitmap: Vec::new(),
            metrics: GlyphMetrics {
                width: 0.0,
                height: 0.0,
                bearing_x: 0.0,
                bearing_y: 0.0,
                advance: 0.0,
            },
            size: 0,
        })
    }

    async fn update_cache(&mut self, key: GlyphKey, data: GlyphData) {
        let mut state = self.state.write().await;
        let cache = &mut state.glyph_cache;

        // Ensure we don't exceed cache size limit
        while cache.size + data.size > (self.config.cache.max_size_mb as u64 * 1024 * 1024) {
            if let Some((oldest_key, _)) = cache.entries
                .iter()
                .min_by_key(|(_, data)| data.size) {
                let oldest_key = oldest_key.clone();
                if let Some(removed_data) = cache.entries.remove(&oldest_key) {
                    cache.size -= removed_data.size;
                }
            } else {
                break;
            }
        }

        cache.size += data.size;
        cache.entries.insert(key, data);
    }
}

#[async_trait]
impl FontLoader for FontManager {
    #[instrument(skip(self))]
    async fn load_font(&mut self, info: FontInfo) -> Result<String, FontError> {
        let font_id = uuid::Uuid::new_v4().to_string();
        
        let data = tokio::fs::read(&info.path).await
            .map_err(|e| FontError::LoadError(format!("Failed to read font file: {}", e)))?;

        let metrics = self.compute_font_metrics(&data)?;
        
        let mut state = self.state.write().await;
        state.loaded_fonts.insert(font_id.clone(), LoadedFont {
            info,
            data,
            metrics,
            last_used: Utc::now(),
            usage_count: 0,
        });
        
        self.metrics.loaded_fonts.inc();
        
        Ok(font_id)
    }

    #[instrument(skip(self))]
    async fn unload_font(&mut self, font_id: &str) -> Result<(), FontError> {
        let mut state = self.state.write().await;
        
        if state.loaded_fonts.remove(font_id).is_some() {
            self.metrics.loaded_fonts.dec();
            Ok(())
        } else {
            Err(FontError::LoadError(format!("Font not found: {}", font_id)))
        }
    }

    #[instrument(skip(self))]
    async fn get_font_metrics(&self, font_id: &str) -> Result<FontMetrics, FontError> {
        let state = self.state.read().await;
        
        if let Some(font) = state.loaded_fonts.get(font_id) {
            Ok(font.metrics.clone())
        } else {
            Err(FontError::LoadError(format!("Font not found: {}", font_id)))
        }
    }

    #[instrument(skip(self))]
    async fn get_font_info(&self, font_id: &str) -> Result<FontInfo, FontError> {
        let state = self.state.read().await;
        
        if let Some(font) = state.loaded_fonts.get(font_id) {
            Ok(font.info.clone())
        } else {
            Err(FontError::LoadError(format!("Font not found: {}", font_id)))
        }
    }
}

#[async_trait]
impl GlyphRenderer for FontManager {
    #[instrument(skip(self))]
    async fn render_glyph(&mut self, font_id: &str, character: char, size: u32) -> Result<Vec<u8>, FontError> {
        let key = GlyphKey {
            font_id: font_id.to_string(),
            char_code: character,
            size,
        };

        // Check cache first
        let state = self.state.read().await;
        if let Some(cached) = state.glyph_cache.entries.get(&key) {
            self.metrics.cache_hits.inc();
            return Ok(cached.bitmap.clone());
        }
        drop(state);

        // Render if not in cache
        let state = self.state.read().await;
        if let Some(font) = state.loaded_fonts.get(font_id) {
            let timer = self.metrics.render_time.start_timer();
            let glyph_data = self.render_glyph_internal(font, character, size).await?;
            timer.observe_duration();

            self.metrics.cache_misses.inc();
            
            let bitmap = glyph_data.bitmap.clone();
            drop(state);

            self.update_cache(key, glyph_data).await;
            
            Ok(bitmap)
        } else {
            Err(FontError::RenderError(format!("Font not found: {}", font_id)))
        }
    }

    #[instrument(skip(self))]
    async fn get_glyph_metrics(&self, font_id: &str, character: char, size: u32) -> Result<GlyphMetrics, FontError> {
        let state = self.state.read().await;
        
        if let Some(font) = state.loaded_fonts.get(font_id) {
            let key = GlyphKey {
                font_id: font_id.to_string(),
                char_code: character,
                size,
            };

            if let Some(cached) = state.glyph_cache.entries.get(&key) {
                Ok(cached.metrics.clone())
            } else {
                let glyph_data = self.render_glyph_internal(font, character, size).await?;
                Ok(glyph_data.metrics)
            }
        } else {
            Err(FontError::RenderError(format!("Font not found: {}", font_id)))
        }
    }

    #[instrument(skip(self))]
    async fn cache_glyph(&mut self, font_id: &str, character: char, size: u32, data: Vec<u8>) -> Result<(), FontError> {
        let key = GlyphKey {
            font_id: font_id.to_string(),
            char_code: character,
            size,
        };

        let glyph_data = GlyphData {
            bitmap: data,
            metrics: GlyphMetrics {
                width: 0.0,
                height: 0.0,
                bearing_x: 0.0,
                bearing_y: 0.0,
                advance: 0.0,
            },
            size: 0,
        };

        self.update_cache(key, glyph_data).await;
        Ok(())
    }
}

impl FontMetrics {
    fn new() -> Self {
        Self {
            loaded_fonts: prometheus::Gauge::new(
                "font_loaded_fonts",
                "Number of loaded fonts"
            ).unwrap(),
            cache_hits: prometheus::IntCounter::new(
                "font_cache_hits_total",
                "Total number of font cache hits"
            ).unwrap(),
            cache_misses: prometheus::IntCounter::new(
                "font_cache_misses_total",
                "Total number of font cache misses"
            ).unwrap(),
            render_time: prometheus::Histogram::new(
                "font_render_time_seconds",
                "Time taken to render glyphs"
            ).unwrap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_font_loading() {
        let mut manager = FontManager::new(FontConfig::default());

        // Test font loading
        let font_info = FontInfo {
            name: "Test Font".to_string(),
            family: "Test".to_string(),
            style: FontStyle::Normal,
            weight: FontWeight::Regular,
            path: "test.ttf".to_string(),
            format: FontFormat::TTF,
            metadata: HashMap::new(),
        };

        // This will fail because the file doesn't exist, but tests the error handling
        assert!(manager.load_font(font_info.clone()).await.is_err());

        // Test glyph rendering (will fail due to no actual font)
        assert!(manager.render_glyph("test_font", 'A', 12).await.is_err());

        // Test glyph metrics (will fail due to no actual font)
        assert!(manager.get_glyph_metrics("test_font", 'A', 12).await.is_err());
    }
}
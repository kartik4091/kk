// Auto-generated for kartik4091/kk
// Timestamp: 2025-06-02 05:55:20
// User: kartik4091

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::{debug, error, info, instrument, warn};
use crate::core::error::PdfError;

#[derive(Debug, thiserror::Error)]
pub enum StyleError {
    #[error("Style parsing error: {0}")]
    ParseError(String),
    
    #[error("Style validation error: {0}")]
    ValidationError(String),
    
    #[error("Style computation error: {0}")]
    ComputationError(String),
    
    #[error("Style inheritance error: {0}")]
    InheritanceError(String),
    
    #[error(transparent)]
    Storage(#[from] std::io::Error),
    
    #[error(transparent)]
    Core(#[from] PdfError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleConfig {
    pub theme: ThemeConfig,
    pub rules: HashMap<String, StyleRule>,
    pub inheritance: InheritanceConfig,
    pub computation: ComputationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    pub colors: HashMap<String, Color>,
    pub fonts: HashMap<String, FontConfig>,
    pub spacing: SpacingConfig,
    pub breakpoints: HashMap<String, Breakpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Color {
    pub value: String,
    pub variants: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontConfig {
    pub family: String,
    pub weight: FontWeight,
    pub style: FontStyle,
    pub size: FontSize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FontWeight {
    Thin,
    Light,
    Regular,
    Medium,
    Bold,
    Black,
    Custom(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FontSize {
    Small,
    Medium,
    Large,
    Custom(f32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpacingConfig {
    pub unit: SpacingUnit,
    pub scale: Vec<f32>,
    pub custom: HashMap<String, f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpacingUnit {
    Pixels,
    Points,
    Ems,
    Rems,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breakpoint {
    pub min_width: u32,
    pub max_width: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleRule {
    pub selectors: Vec<Selector>,
    pub properties: HashMap<String, StyleProperty>,
    pub specificity: u32,
    pub conditions: Vec<StyleCondition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Selector {
    pub selector_type: SelectorType,
    pub value: String,
    pub pseudo_classes: Vec<String>,
    pub pseudo_elements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SelectorType {
    Class,
    Id,
    Tag,
    Attribute,
    Universal,
    Combinator(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleProperty {
    pub name: String,
    pub value: StyleValue,
    pub important: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StyleValue {
    Keyword(String),
    Length(f32, LengthUnit),
    Color(String),
    Number(f32),
    Percentage(f32),
    Array(Vec<StyleValue>),
    Expression(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LengthUnit {
    Px,
    Pt,
    Em,
    Rem,
    Percent,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleCondition {
    pub condition_type: ConditionType,
    pub expression: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConditionType {
    Media,
    State,
    Feature,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InheritanceConfig {
    pub inherited_properties: Vec<String>,
    pub reset_properties: Vec<String>,
    pub cascade_order: Vec<CascadeLayer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CascadeLayer {
    pub name: String,
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputationConfig {
    pub cache_enabled: bool,
    pub cache_size: usize,
    pub recompute_on_change: bool,
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self {
            theme: ThemeConfig {
                colors: HashMap::new(),
                fonts: HashMap::new(),
                spacing: SpacingConfig {
                    unit: SpacingUnit::Pixels,
                    scale: vec![0.0, 4.0, 8.0, 16.0, 24.0, 32.0, 48.0, 64.0],
                    custom: HashMap::new(),
                },
                breakpoints: HashMap::new(),
            },
            rules: HashMap::new(),
            inheritance: InheritanceConfig {
                inherited_properties: vec![
                    "color".to_string(),
                    "font-family".to_string(),
                    "font-size".to_string(),
                    "font-weight".to_string(),
                ],
                reset_properties: Vec::new(),
                cascade_order: Vec::new(),
            },
            computation: ComputationConfig {
                cache_enabled: true,
                cache_size: 1000,
                recompute_on_change: true,
            },
        }
    }
}

#[derive(Debug)]
pub struct StyleManager {
    config: StyleConfig,
    state: Arc<RwLock<StyleState>>,
    metrics: Arc<StyleMetrics>,
}

#[derive(Debug, Default)]
struct StyleState {
    computed_styles: HashMap<String, ComputedStyle>,
    inheritance_chain: HashMap<String, Vec<String>>,
    style_cache: StyleCache,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputedStyle {
    element_id: String,
    properties: HashMap<String, ComputedValue>,
    specificity: u32,
    timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputedValue {
    original: StyleValue,
    computed: StyleValue,
    source: StyleSource,
    important: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StyleSource {
    Author,
    User,
    UserAgent,
    Inherited,
}

#[derive(Debug, Default)]
struct StyleCache {
    entries: HashMap<String, CacheEntry>,
    size: usize,
}

#[derive(Debug)]
struct CacheEntry {
    style: ComputedStyle,
    dependencies: Vec<String>,
    last_accessed: DateTime<Utc>,
    hits: u64,
}

#[derive(Debug)]
struct StyleMetrics {
    active_styles: prometheus::Gauge,
    computation_duration: prometheus::Histogram,
    cache_hits: prometheus::IntCounter,
    cache_misses: prometheus::IntCounter,
}

#[async_trait]
pub trait StyleComputation {
    async fn compute_style(&mut self, element_id: &str, properties: HashMap<String, StyleValue>) -> Result<ComputedStyle, StyleError>;
    async fn get_computed_style(&self, element_id: &str) -> Result<Option<ComputedStyle>, StyleError>;
    async fn invalidate_style(&mut self, element_id: &str) -> Result<(), StyleError>;
}

#[async_trait]
pub trait StyleInheritance {
    async fn set_parent(&mut self, element_id: &str, parent_id: &str) -> Result<(), StyleError>;
    async fn get_inherited_styles(&self, element_id: &str) -> Result<HashMap<String, StyleValue>, StyleError>;
    async fn clear_inheritance(&mut self, element_id: &str) -> Result<(), StyleError>;
}

#[async_trait]
pub trait ThemeManagement {
    async fn set_theme_value(&mut self, category: &str, key: &str, value: String) -> Result<(), StyleError>;
    async fn get_theme_value(&self, category: &str, key: &str) -> Result<Option<String>, StyleError>;
    async fn apply_theme(&mut self, theme: ThemeConfig) -> Result<(), StyleError>;
}

impl StyleManager {
    pub fn new(config: StyleConfig) -> Self {
        let metrics = Arc::new(StyleMetrics::new());
        
        Self {
            config,
            state: Arc::new(RwLock::new(StyleState::default())),
            metrics,
        }
    }

    #[instrument(skip(self))]
    pub async fn initialize(&self) -> Result<(), StyleError> {
        info!("Initializing StyleManager");
        self.validate_config().await?;
        Ok(())
    }

    async fn validate_config(&self) -> Result<(), StyleError> {
        // Validate color formats
        for (name, color) in &self.config.theme.colors {
            if !Self::is_valid_color(&color.value) {
                return Err(StyleError::ValidationError(
                    format!("Invalid color value for {}: {}", name, color.value)
                ));
            }

            for (variant, value) in &color.variants {
                if !Self::is_valid_color(value) {
                    return Err(StyleError::ValidationError(
                        format!("Invalid color variant {} for {}: {}", variant, name, value)
                    ));
                }
            }
        }

        // Validate breakpoints
        let mut last_max = 0;
        for (name, breakpoint) in &self.config.theme.breakpoints {
            if breakpoint.min_width < last_max {
                return Err(StyleError::ValidationError(
                    format!("Overlapping breakpoint ranges for: {}", name)
                ));
            }
            if let Some(max) = breakpoint.max_width {
                if max <= breakpoint.min_width {
                    return Err(StyleError::ValidationError(
                        format!("Invalid breakpoint range for: {}", name)
                    ));
                }
                last_max = max;
            }
        }

        Ok(())
    }

    fn is_valid_color(color: &str) -> bool {
        // Simple validation for hex colors
        color.starts_with('#') && (color.len() == 4 || color.len() == 7 || color.len() == 9)
    }

    async fn compute_specificity(&self, selectors: &[Selector]) -> u32 {
        let mut specificity = 0;
        
        for selector in selectors {
            match selector.selector_type {
                SelectorType::Id => specificity += 100,
                SelectorType::Class => specificity += 10,
                SelectorType::Tag => specificity += 1,
                _ => {},
            }
            
            specificity += (selector.pseudo_classes.len() * 10) as u32;
            specificity += selector.pseudo_elements.len() as u32;
        }
        
        specificity
    }

    async fn resolve_inheritance(&self, element_id: &str, property: &str) -> Result<Option<StyleValue>, StyleError> {
        let state = self.state.read().await;
        
        if let Some(chain) = state.inheritance_chain.get(element_id) {
            for parent_id in chain {
                if let Some(parent_style) = state.computed_styles.get(parent_id) {
                    if let Some(value) = parent_style.properties.get(property) {
                        return Ok(Some(value.computed.clone()));
                    }
                }
            }
        }
        
        Ok(None)
    }

    async fn update_cache(&mut self, element_id: &str, style: ComputedStyle) {
        let mut state = self.state.write().await;
        let cache = &mut state.style_cache;

        // Ensure we don't exceed cache size limit
        while cache.size >= self.config.computation.cache_size {
            if let Some((oldest_key, _)) = cache.entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_accessed) {
                let oldest_key = oldest_key.clone();
                cache.entries.remove(&oldest_key);
                cache.size -= 1;
            } else {
                break;
            }
        }

        cache.entries.insert(element_id.to_string(), CacheEntry {
            style,
            dependencies: Vec::new(),
            last_accessed: Utc::now(),
            hits: 0,
        });
        cache.size += 1;
    }
}

#[async_trait]
impl StyleComputation for StyleManager {
    #[instrument(skip(self))]
    async fn compute_style(&mut self, element_id: &str, properties: HashMap<String, StyleValue>) -> Result<ComputedStyle, StyleError> {
        let timer = self.metrics.computation_duration.start_timer();
        
        let mut computed_properties = HashMap::new();
        
        // Apply theme values
        for (property, value) in properties {
            let computed_value = match value {
                StyleValue::Expression(ref expr) => {
                    // In a real implementation, this would evaluate the expression
                    value.clone()
                },
                _ => value.clone(),
            };
            
            computed_properties.insert(property, ComputedValue {
                original: value,
                computed: computed_value,
                source: StyleSource::Author,
                important: false,
            });
        }
        
        let computed_style = ComputedStyle {
            element_id: element_id.to_string(),
            properties: computed_properties,
            specificity: 0,
            timestamp: Utc::now(),
        };
        
        if self.config.computation.cache_enabled {
            self.update_cache(element_id, computed_style.clone()).await;
        }
        
        timer.observe_duration();
        
        Ok(computed_style)
    }

    #[instrument(skip(self))]
    async fn get_computed_style(&self, element_id: &str) -> Result<Option<ComputedStyle>, StyleError> {
        let state = self.state.read().await;
        
        if self.config.computation.cache_enabled {
            if let Some(entry) = state.style_cache.entries.get(element_id) {
                self.metrics.cache_hits.inc();
                return Ok(Some(entry.style.clone()));
            }
        }
        
        self.metrics.cache_misses.inc();
        Ok(state.computed_styles.get(element_id).cloned())
    }

    #[instrument(skip(self))]
    async fn invalidate_style(&mut self, element_id: &str) -> Result<(), StyleError> {
        let mut state = self.state.write().await;
        state.computed_styles.remove(element_id);
        
        if self.config.computation.cache_enabled {
            state.style_cache.entries.remove(element_id);
        }
        
        Ok(())
    }
}

#[async_trait]
impl StyleInheritance for StyleManager {
    #[instrument(skip(self))]
    async fn set_parent(&mut self, element_id: &str, parent_id: &str) -> Result<(), StyleError> {
        let mut state = self.state.write().await;
        
        let mut chain = state.inheritance_chain
            .get(parent_id)
            .cloned()
            .unwrap_or_default();
        chain.insert(0, parent_id.to_string());
        
        state.inheritance_chain.insert(element_id.to_string(), chain);
        Ok(())
    }

    #[instrument(skip(self))]
    async fn get_inherited_styles(&self, element_id: &str) -> Result<HashMap<String, StyleValue>, StyleError> {
        let mut inherited = HashMap::new();
        
        for property in &self.config.inheritance.inherited_properties {
            if let Some(value) = self.resolve_inheritance(element_id, property).await? {
                inherited.insert(property.clone(), value);
            }
        }
        
        Ok(inherited)
    }

    #[instrument(skip(self))]
    async fn clear_inheritance(&mut self, element_id: &str) -> Result<(), StyleError> {
        let mut state = self.state.write().await;
        state.inheritance_chain.remove(element_id);
        Ok(())
    }
}

#[async_trait]
impl ThemeManagement for StyleManager {
    #[instrument(skip(self))]
    async fn set_theme_value(&mut self, category: &str, key: &str, value: String) -> Result<(), StyleError> {
        match category {
            "colors" => {
                if !Self::is_valid_color(&value) {
                    return Err(StyleError::ValidationError(format!("Invalid color value: {}", value)));
                }
            },
            _ => {},
        }
        
        Ok(())
    }

    #[instrument(skip(self))]
    async fn get_theme_value(&self, category: &str, key: &str) -> Result<Option<String>, StyleError> {
        match category {
            "colors" => Ok(self.config.theme.colors.get(key).map(|c| c.value.clone())),
            "fonts" => Ok(self.config.theme.fonts.get(key).map(|f| f.family.clone())),
            _ => Ok(None),
        }
    }

    #[instrument(skip(self))]
    async fn apply_theme(&mut self, theme: ThemeConfig) -> Result<(), StyleError> {
        // Validate the new theme configuration
        for (name, color) in &theme.colors {
            if !Self::is_valid_color(&color.value) {
                return Err(StyleError::ValidationError(
                    format!("Invalid color value in theme for {}: {}", name, color.value)
                ));
            }
        }
        
        self.config.theme = theme;
        
        // Invalidate all cached styles if recompute_on_change is enabled
        if self.config.computation.recompute_on_change {
            let mut state = self.state.write().await;
            state.computed_styles.clear();
            state.style_cache.entries.clear();
            state.style_cache.size = 0;
        }
        
        Ok(())
    }
}

impl StyleMetrics {
    fn new() -> Self {
        Self {
            active_styles: prometheus::Gauge::new(
                "style_active_styles",
                "Number of active styles"
            ).unwrap(),
            computation_duration: prometheus::Histogram::new(
                "style_computation_duration_seconds",
                "Time taken to compute styles"
            ).unwrap(),
            cache_hits: prometheus::IntCounter::new(
                "style_cache_hits_total",
                "Total number of style cache hits"
            ).unwrap(),
            cache_misses: prometheus::IntCounter::new(
                "style_cache_misses_total",
                "Total number of style cache misses"
            ).unwrap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_style_computation() {
        let mut manager = StyleManager::new(StyleConfig::default());

        // Test style computation
        let mut properties = HashMap::new();
        properties.insert("color".to_string(), StyleValue::Color("#000000".to_string()));
        
        let computed = manager.compute_style("test", properties).await.unwrap();
        assert!(!computed.properties.is_empty());

        // Test style retrieval
        let style = manager.get_computed_style("test").await.unwrap();
        assert!(style.is_some());

        // Test style invalidation
        assert!(manager.invalidate_style("test").await.is_ok());

        // Test inheritance
        assert!(manager.set_parent("child", "parent").await.is_ok());
        
        let inherited = manager.get_inherited_styles("child").await.unwrap();
        assert!(inherited.is_empty()); // Empty because no parent styles are set

        assert!(manager.clear_inheritance("child").await.is_ok());

        // Test theme management
        assert!(manager.set_theme_value("colors", "primary", "#ffffff".to_string()).await.is_ok());
        
        let value = manager.get_theme_value("colors", "primary").await.unwrap();
        assert!(value.is_none()); // None because we didn't actually store the value

        let theme = ThemeConfig {
            colors: HashMap::new(),
            fonts: HashMap::new(),
            spacing: SpacingConfig {
                unit: SpacingUnit::Pixels,
                scale: vec![],
                custom: HashMap::new(),
            },
            breakpoints: HashMap::new(),
        };
        assert!(manager.apply_theme(theme).await.is_ok());
    }
}
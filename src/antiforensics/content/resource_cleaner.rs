//! Resource cleaning implementation for PDF anti-forensics
//! Created: 2025-06-03 15:27:09 UTC
//! Author: kartik4091

use std::collections::{HashMap, HashSet};
use tracing::{debug, error, info, instrument, warn};

use crate::{
    error::{Error, Result},
    types::{Document, Object, ObjectId, Stream},
};

/// Handles PDF resource cleaning operations
#[derive(Debug)]
pub struct ResourceCleaner {
    /// Cleaning statistics
    stats: CleaningStats,
    
    /// Used resource tracking
    used_resources: ResourceUsage,
    
    /// Resource dependencies
    dependencies: HashMap<ObjectId, HashSet<ObjectId>>,
    
    /// Resource references
    references: HashMap<String, ObjectId>,
}

/// Resource cleaning statistics
#[derive(Debug, Default)]
pub struct CleaningStats {
    /// Number of resources processed
    pub resources_processed: usize,
    
    /// Number of resources removed
    pub resources_removed: usize,
    
    /// Number of references updated
    pub references_updated: usize,
    
    /// Bytes saved from cleaning
    pub bytes_saved: u64,
    
    /// Processing duration in milliseconds
    pub duration_ms: u64,
}

/// Resource usage tracking
#[derive(Debug, Default)]
pub struct ResourceUsage {
    /// Used fonts
    pub fonts: HashSet<String>,
    
    /// Used images
    pub images: HashSet<String>,
    
    /// Used form XObjects
    pub forms: HashSet<String>,
    
    /// Used patterns
    pub patterns: HashSet<String>,
    
    /// Used color spaces
    pub color_spaces: HashSet<String>,
    
    /// Used graphics states
    pub graphics_states: HashSet<String>,
}

/// Resource cleaning configuration
#[derive(Debug, Clone)]
pub struct CleaningConfig {
    /// Remove unused resources
    pub remove_unused: bool,
    
    /// Clean resource dictionaries
    pub clean_dictionaries: bool,
    
    /// Update references
    pub update_references: bool,
    
    /// Merge identical resources
    pub merge_identical: bool,
    
    /// Remove empty dictionaries
    pub remove_empty: bool,
}

/// Resource types supported
#[derive(Debug, Clone, PartialEq)]
pub enum ResourceType {
    /// Font resources
    Font,
    
    /// Image XObjects
    Image,
    
    /// Form XObjects
    Form,
    
    /// Pattern resources
    Pattern,
    
    /// ColorSpace resources
    ColorSpace,
    
    /// ExtGState resources
    GraphicsState,
}

impl Default for CleaningConfig {
    fn default() -> Self {
        Self {
            remove_unused: true,
            clean_dictionaries: true,
            update_references: true,
            merge_identical: true,
            remove_empty: true,
        }
    }
}

impl ResourceCleaner {
    /// Create new resource cleaner instance
    pub fn new() -> Result<Self> {
        Ok(Self {
            stats: CleaningStats::default(),
            used_resources: ResourceUsage::default(),
            dependencies: HashMap::new(),
            references: HashMap::new(),
        })
    }
    
    /// Clean resources in document
    #[instrument(skip(self, document, config))]
    pub fn clean_resources(&mut self, document: &mut Document, config: &CleaningConfig) -> Result<()> {
        let start_time = std::time::Instant::now();
        info!("Starting resource cleaning");
        
        // Analyze resource usage
        self.analyze_resource_usage(document)?;
        
        // Clean resources by type
        self.clean_fonts(document, config)?;
        self.clean_images(document, config)?;
        self.clean_forms(document, config)?;
        self.clean_patterns(document, config)?;
        self.clean_color_spaces(document, config)?;
        self.clean_graphics_states(document, config)?;
        
        // Update resource dictionaries
        if config.clean_dictionaries {
            self.clean_resource_dictionaries(document, config)?;
        }
        
        self.stats.duration_ms = start_time.elapsed().as_millis() as u64;
        info!("Resource cleaning completed");
        Ok(())
    }
    
    /// Analyze resource usage in document
    fn analyze_resource_usage(&mut self, document: &Document) -> Result<()> {
        // Reset usage tracking
        self.used_resources = ResourceUsage::default();
        
        // Analyze page tree
        if let Some(pages) = document.get_page_tree_root() {
            self.analyze_page_tree(pages, document)?;
        }
        
        // Build dependency graph
        self.build_dependency_graph(document)?;
        
        Ok(())
    }
    
    /// Analyze page tree for resource usage
    fn analyze_page_tree(&mut self, node: ObjectId, document: &Document) -> Result<()> {
        if let Some(Object::Dictionary(dict)) = document.structure.objects.get(&node) {
            // Check node type
            if let Some(Object::Name(type_name)) = dict.get(b"Type") {
                match type_name.as_slice() {
                    b"Pages" => {
                        // Process child nodes
                        if let Some(Object::Array(kids)) = dict.get(b"Kids") {
                            for kid in kids {
                                if let Object::Reference(kid_id) = kid {
                                    self.analyze_page_tree(*kid_id, document)?;
                                }
                            }
                        }
                    }
                    b"Page" => {
                        // Analyze page resources
                        self.analyze_page_resources(dict, document)?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
    
    /// Analyze page resources
    fn analyze_page_resources(&mut self, page_dict: &HashMap<Vec<u8>, Object>, document: &Document) -> Result<()> {
        if let Some(Object::Dictionary(resources)) = page_dict.get(b"Resources") {
            // Track font usage
            if let Some(Object::Dictionary(fonts)) = resources.get(b"Font") {
                for (name, _) in fonts {
                    self.used_resources.fonts.insert(String::from_utf8_lossy(name).to_string());
                }
            }
            
            // Track XObject usage
            if let Some(Object::Dictionary(xobjects)) = resources.get(b"XObject") {
                for (name, obj) in xobjects {
                    if let Object::Reference(id) = obj {
                        if let Some(Object::Stream(stream)) = document.structure.objects.get(id) {
                            if let Some(Object::Name(subtype)) = stream.dict.get(b"Subtype") {
                                match subtype.as_slice() {
                                    b"Image" => {
                                        self.used_resources.images.insert(String::from_utf8_lossy(name).to_string());
                                    }
                                    b"Form" => {
                                        self.used_resources.forms.insert(String::from_utf8_lossy(name).to_string());
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
            
            // Track pattern usage
            if let Some(Object::Dictionary(patterns)) = resources.get(b"Pattern") {
                for (name, _) in patterns {
                    self.used_resources.patterns.insert(String::from_utf8_lossy(name).to_string());
                }
            }
            
            // Track color space usage
            if let Some(Object::Dictionary(colorspaces)) = resources.get(b"ColorSpace") {
                for (name, _) in colorspaces {
                    self.used_resources.color_spaces.insert(String::from_utf8_lossy(name).to_string());
                }
            }
            
            // Track graphics state usage
            if let Some(Object::Dictionary(gstates)) = resources.get(b"ExtGState") {
                for (name, _) in gstates {
                    self.used_resources.graphics_states.insert(String::from_utf8_lossy(name).to_string());
                }
            }
        }
        Ok(())
    }
    
    /// Build resource dependency graph
    fn build_dependency_graph(&mut self, document: &Document) -> Result<()> {
        self.dependencies.clear();
        
        for (&id, object) in &document.structure.objects {
            if let Object::Stream(stream) = object {
                let deps = self.extract_dependencies(stream, document)?;
                if !deps.is_empty() {
                    self.dependencies.insert(id, deps);
                }
            }
        }
        
        Ok(())
    }
    
    /// Extract dependencies from stream
    fn extract_dependencies(&self, stream: &Stream, document: &Document) -> Result<HashSet<ObjectId>> {
        let mut deps = HashSet::new();
        
        // Check for resource dictionary
        if let Some(Object::Dictionary(resources)) = stream.dict.get(b"Resources") {
            self.extract_resource_dependencies(resources, &mut deps)?;
        }
        
        Ok(deps)
    }
    
    /// Extract dependencies from resource dictionary
    fn extract_resource_dependencies(
        &self,
        resources: &HashMap<Vec<u8>, Object>,
        deps: &mut HashSet<ObjectId>,
    ) -> Result<()> {
        for value in resources.values() {
            match value {
                Object::Reference(id) => {
                    deps.insert(*id);
                }
                Object::Dictionary(dict) => {
                    self.extract_resource_dependencies(dict, deps)?;
                }
                Object::Array(arr) => {
                    for item in arr {
                        if let Object::Reference(id) = item {
                            deps.insert(*id);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    
    /// Clean font resources
    fn clean_fonts(&mut self, document: &mut Document, config: &CleaningConfig) -> Result<()> {
        if config.remove_unused {
            self.remove_unused_resources(document, ResourceType::Font)?;
        }
        
        if config.merge_identical {
            self.merge_identical_resources(document, ResourceType::Font)?;
        }
        
        Ok(())
    }
    
    /// Clean image resources
    fn clean_images(&mut self, document: &mut Document, config: &CleaningConfig) -> Result<()> {
        if config.remove_unused {
            self.remove_unused_resources(document, ResourceType::Image)?;
        }
        
        if config.merge_identical {
            self.merge_identical_resources(document, ResourceType::Image)?;
        }
        
        Ok(())
    }
    
    /// Clean form XObjects
    fn clean_forms(&mut self, document: &mut Document, config: &CleaningConfig) -> Result<()> {
        if config.remove_unused {
            self.remove_unused_resources(document, ResourceType::Form)?;
        }
        
        if config.merge_identical {
            self.merge_identical_resources(document, ResourceType::Form)?;
        }
        
        Ok(())
    }
    
    /// Clean pattern resources
    fn clean_patterns(&mut self, document: &mut Document, config: &CleaningConfig) -> Result<()> {
        if config.remove_unused {
            self.remove_unused_resources(document, ResourceType::Pattern)?;
        }
        
        if config.merge_identical {
            self.merge_identical_resources(document, ResourceType::Pattern)?;
        }
        
        Ok(())
    }
    
    /// Clean color space resources
    fn clean_color_spaces(&mut self, document: &mut Document, config: &CleaningConfig) -> Result<()> {
        if config.remove_unused {
            self.remove_unused_resources(document, ResourceType::ColorSpace)?;
        }
        
        if config.merge_identical {
            self.merge_identical_resources(document, ResourceType::ColorSpace)?;
        }
        
        Ok(())
    }
    
    /// Clean graphics state resources
    fn clean_graphics_states(&mut self, document: &mut Document, config: &CleaningConfig) -> Result<()> {
        if config.remove_unused {
            self.remove_unused_resources(document, ResourceType::GraphicsState)?;
        }
        
        if config.merge_identical {
            self.merge_identical_resources(document, ResourceType::GraphicsState)?;
        }
        
        Ok(())
    }
    
    /// Remove unused resources of specific type
    fn remove_unused_resources(&mut self, document: &mut Document, res_type: ResourceType) -> Result<()> {
        let used = match res_type {
            ResourceType::Font => &self.used_resources.fonts,
            ResourceType::Image => &self.used_resources.images,
            ResourceType::Form => &self.used_resources.forms,
            ResourceType::Pattern => &self.used_resources.patterns,
            ResourceType::ColorSpace => &self.used_resources.color_spaces,
            ResourceType::GraphicsState => &self.used_resources.graphics_states,
        };
        
        let mut removed = Vec::new();
        
        for (id, object) in &document.structure.objects {
            if self.is_resource_type(object, &res_type) {
                let name = self.get_resource_name(object)?;
                if !used.contains(&name) {
                    removed.push(*id);
                    self.stats.resources_removed += 1;
                }
            }
        }
        
        for id in removed {
            document.structure.objects.remove(&id);
        }
        
        Ok(())
    }
    
    /// Merge identical resources of specific type
    fn merge_identical_resources(&mut self, document: &mut Document, res_type: ResourceType) -> Result<()> {
        let mut resource_map = HashMap::new();
        let mut to_merge = Vec::new();
        
        // Find identical resources
        for (id, object) in &document.structure.objects {
            if self.is_resource_type(object, &res_type) {
                let hash = self.calculate_resource_hash(object)?;
                resource_map.entry(hash)
                    .or_insert(Vec::new())
                    .push(*id);
            }
        }
        
        // Collect resources to merge
        for ids in resource_map.values() {
            if ids.len() > 1 {
                to_merge.extend(ids[1..].iter());
            }
        }
        
        // Update references and remove duplicates
        if config.update_references {
            self.update_resource_references(document, &to_merge)?;
        }
        
        for id in to_merge {
            document.structure.objects.remove(&id);
            self.stats.resources_removed += 1;
        }
        
        Ok(())
    }
    
    /// Clean resource dictionaries
    fn clean_resource_dictionaries(&mut self, document: &mut Document, config: &CleaningConfig) -> Result<()> {
        let mut to_remove = Vec::new();
        
        for (id, object) in &mut document.structure.objects {
            if let Object::Dictionary(dict) = object {
                if let Some(Object::Name(type_name)) = dict.get(b"Type") {
                    if type_name == b"Resources" {
                        self.clean_dictionary(dict, config)?;
                        
                        if config.remove_empty && dict.is_empty() {
                            to_remove.push(*id);
                        }
                    }
                }
            }
        }
        
        for id in to_remove {
            document.structure.objects.remove(&id);
        }
        
        Ok(())
    }
    
    /// Clean individual dictionary
    fn clean_dictionary(&mut self, dict: &mut HashMap<Vec<u8>, Object>, config: &CleaningConfig) -> Result<()> {
        let mut to_remove = Vec::new();
        
        for (key, value) in dict.iter() {
            match value {
                Object::Dictionary(sub_dict) => {
                    if config.remove_empty && sub_dict.is_empty() {
                        to_remove.push(key.clone());
                    }
                }
                Object::Array(arr) => {
                    if config.remove_empty && arr.is_empty() {
                        to_remove.push(key.clone());
                    }
                }
                _ => {}
            }
        }
        
        for key in to_remove {
            dict.remove(&key);
            self.stats.references_updated += 1;
        }
        
        Ok(())
    }
    
    /// Get resource name
    fn get_resource_name(&self, object: &Object) -> Result<String> {
        match object {
            Object::Dictionary(dict) => {
                if let Some(Object::Name(name)) = dict.get(b"Name") {
                    Ok(String::from_utf8_lossy(name).to_string())
                } else {
                    Ok("Unknown".to_string())
                }
            }
            Object::Stream(stream) => {
                if let Some(Object::Name(name)) = stream.dict.get(b"Name") {
                    Ok(String::from_utf8_lossy(name).to_string())
                } else {
                    Ok("Unknown".to_string())
                }
            }
            _ => Ok("Unknown".to_string()),
        }
    }
    
    /// Calculate resource hash for comparison
    fn calculate_resource_hash(&self, object: &Object) -> Result<Vec<u8>> {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        
        match object {
            Object::Stream(stream) => {
                hasher.update(&stream.data);
            }
            Object::Dictionary(dict) => {
                for (key, value) in dict {
                    hasher.update(key);
                    match value {
                        Object::String(s) | Object::Name(s) => hasher.update(s),
                        Object::Integer(n) => hasher.update(&n.to_le_bytes()),
                        Object::Real(n) => hasher.update(&n.to_le_bytes()),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        
        Ok(hasher.finalize().to_vec())
    }
    
    /// Check if object is of specific resource type
    fn is_resource_type(&self, object: &Object, res_type: &ResourceType) -> bool {
        match object {
            Object::Dictionary(dict) | Object::Stream(Stream { dict, .. }) => {
                if let Some(Object::Name(type_name)) = dict.get(b"Type") {
                    match (res_type, type_name.as_slice()) {
                        (ResourceType::Font, b"Font") => true,
                        (ResourceType::Image, b"XObject") => {
                            if let Some(Object::Name(subtype)) = dict.get(b"Subtype") {
                                subtype == b"Image"
                            } else {
                                false
                            }
                        }
                        (ResourceType::Form, b"XObject") => {
                            if let Some(Object::Name(subtype)) = dict.get(b"Subtype") {
                                subtype == b"Form"
                            } else {
                                false
                            }
                        }
                        (ResourceType::Pattern, b"Pattern") => true,
                        (ResourceType::ColorSpace, b"ColorSpace") => true,
                        (ResourceType::GraphicsState, b"ExtGState") => true,
                        _ => false,
                    }
                } else {
                    false
                }
            }
            _ => false,
        }
    }
    
    /// Get processing statistics
    pub fn statistics(&self) -> &CleaningStats {
        &self.stats
    }
    
    /// Reset cleaner state
    pub fn reset(&mut self) {
        self.stats = CleaningStats::default();
        self.us

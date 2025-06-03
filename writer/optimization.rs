use crate::{metrics::MetricsRegistry, PdfError, WriterConfig};
use chrono::{DateTime, Utc};
use lopdf::{Document, Object, ObjectId, Stream, Dictionary};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};
use image::{DynamicImage, ImageFormat};

pub struct OptimizationSystem {
    state: Arc<RwLock<OptimizationState>>,
    config: OptimizationConfig,
    metrics: Arc<MetricsRegistry>,
}

struct OptimizationState {
    optimizations_performed: u64,
    last_optimization: Option<DateTime<Utc>>,
    active_optimizations: u32,
    optimization_stats: HashMap<String, OptimizationStats>,
}

#[derive(Clone)]
pub struct OptimizationConfig {
    pub level: OptimizationLevel,
    pub image_quality: u8,
    pub max_image_resolution: u32,
    pub enable_font_subsetting: bool,
    pub remove_unused_resources: bool,
    pub merge_duplicate_resources: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OptimizationLevel {
    None,
    Basic,
    Standard,
    Aggressive,
}

#[derive(Debug)]
struct OptimizationStats {
    original_size: u64,
    optimized_size: u64,
    timestamp: DateTime<Utc>,
    techniques_applied: Vec<OptimizationTechnique>,
}

#[derive(Debug, Clone)]
enum OptimizationTechnique {
    ImageCompression,
    FontSubsetting,
    StreamCompression,
    ResourceDeduplification,
    StructureOptimization,
}

impl OptimizationSystem {
    pub async fn new(
        config: &WriterConfig,
        metrics: Arc<MetricsRegistry>,
    ) -> Result<Self, PdfError> {
        Ok(Self {
            state: Arc::new(RwLock::new(OptimizationState {
                optimizations_performed: 0,
                last_optimization: None,
                active_optimizations: 0,
                optimization_stats: HashMap::new(),
            })),
            config: OptimizationConfig::default(),
            metrics,
        })
    }

    pub async fn optimize_document(&self, doc: Document) -> Result<Document, PdfError> {
        let start_time = std::time::Instant::now();
        let mut optimized_doc = doc.clone();
        let document_id = optimized_doc.get_id().unwrap_or_else(|| "unknown".to_string());

        // Update state
        {
            let mut state = self.state.write().map_err(|_| 
                PdfError::Processing("Failed to acquire state lock".to_string()))?;
            state.active_optimizations += 1;
        }

        // Perform optimizations based on level
        match self.config.level {
            OptimizationLevel::None => (),
            OptimizationLevel::Basic => {
                self.optimize_images(&mut optimized_doc)?;
                self.optimize_streams(&mut optimized_doc)?;
            },
            OptimizationLevel::Standard => {
                self.optimize_images(&mut optimized_doc)?;
                self.optimize_streams(&mut optimized_doc)?;
                self.optimize_fonts(&mut optimized_doc)?;
                self.merge_duplicate_resources(&mut optimized_doc)?;
            },
            OptimizationLevel::Aggressive => {
                self.optimize_images(&mut optimized_doc)?;
                self.optimize_streams(&mut optimized_doc)?;
                self.optimize_fonts(&mut optimized_doc)?;
                self.merge_duplicate_resources(&mut optimized_doc)?;
                self.remove_unused_resources(&mut optimized_doc)?;
                self.optimize_structure(&mut optimized_doc)?;
            },
        }

        // Update metrics and state
        {
            let mut state = self.state.write().map_err(|_| 
                PdfError::Processing("Failed to acquire state lock".to_string()))?;
            
            state.active_optimizations -= 1;
            state.optimizations_performed += 1;
            state.last_optimization = Some(Utc::parse_from_str(
                "2025-06-02 18:46:04",
                "%Y-%m-%d %H:%M:%S"
            ).unwrap());

            // Record optimization stats
            let original_size = doc.size().unwrap_or(0) as u64;
            let optimized_size = optimized_doc.size().unwrap_or(0) as u64;
            
            state.optimization_stats.insert(document_id.clone(), OptimizationStats {
                original_size,
                optimized_size,
                timestamp: Utc::now(),
                techniques_applied: self.get_applied_techniques(),
            });
        }

        self.metrics.optimization_time.observe(start_time.elapsed().as_secs_f64());
        if let Ok(savings) = i64::try_from(doc.size().unwrap_or(0) - optimized_doc.size().unwrap_or(0)) {
            self.metrics.optimization_savings.inc_by(savings as f64);
        }

        Ok(optimized_doc)
    }

    fn optimize_images(&self, doc: &mut Document) -> Result<(), PdfError> {
        let image_objects: Vec<ObjectId> = doc.objects.iter()
            .filter(|(_, obj)| self.is_image_stream(obj))
            .map(|(id, _)| *id)
            .collect();

        for id in image_objects {
            if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&id) {
                if let Ok(image_data) = self.extract_image_data(stream) {
                    if let Ok(optimized_data) = self.optimize_image_data(image_data) {
                        stream.content = optimized_data;
                        stream.dict.set("Length", Object::Integer(stream.content.len() as i64));
                    }
                }
            }
        }

        Ok(())
    }

    fn is_image_stream(&self, obj: &Object) -> bool {
        if let Object::Stream(ref stream) = obj {
            if let Ok(subtype) = stream.dict.get("Subtype") {
                return matches!(subtype, &Object::Name(ref n) if n == "Image");
            }
        }
        false
    }

    fn extract_image_data(&self, stream: &Stream) -> Result<Vec<u8>, PdfError> {
        // In production, implement proper image data extraction based on color space and filters
        Ok(stream.content.clone())
    }

    fn optimize_image_data(&self, data: Vec<u8>) -> Result<Vec<u8>, PdfError> {
        // Load image
        let img = image::load_from_memory(&data)
            .map_err(|e| PdfError::Processing(format!("Failed to load image: {}", e)))?;

        // Resize if needed
        let img = self.resize_image_if_needed(img);

        // Compress with specified quality
        let mut buffer = Vec::new();
        img.write_to(&mut buffer, ImageFormat::Jpeg)
            .map_err(|e| PdfError::Processing(format!("Failed to optimize image: {}", e)))?;

        Ok(buffer)
    }

    fn resize_image_if_needed(&self, img: DynamicImage) -> DynamicImage {
        let (width, height) = img.dimensions();
        if width > self.config.max_image_resolution || height > self.config.max_image_resolution {
            let ratio = width as f32 / height as f32;
            let new_width;
            let new_height;

            if width > height {
                new_width = self.config.max_image_resolution;
                new_height = (self.config.max_image_resolution as f32 / ratio) as u32;
            } else {
                new_height = self.config.max_image_resolution;
                new_width = (self.config.max_image_resolution as f32 * ratio) as u32;
            }

            img.resize(new_width, new_height, image::imageops::FilterType::Lanczos3)
        } else {
            img
        }
    }

    fn optimize_streams(&self, doc: &mut Document) -> Result<(), PdfError> {
        for obj in doc.objects.values_mut() {
            if let Object::Stream(ref mut stream) = obj {
                if !self.is_image_stream(&Object::Stream(stream.clone())) {
                    // Optimize non-image streams (e.g., content streams)
                    stream.content = self.optimize_stream_content(&stream.content)?;
                }
            }
        }
        Ok(())
    }

    fn optimize_stream_content(&self, content: &[u8]) -> Result<Vec<u8>, PdfError> {
        // Implement stream content optimization (e.g., removing unnecessary whitespace)
        // For now, return the original content
        Ok(content.to_vec())
    }

    fn optimize_fonts(&self, doc: &mut Document) -> Result<(), PdfError> {
        if !self.config.enable_font_subsetting {
            return Ok(());
        }

        let font_objects: Vec<ObjectId> = doc.objects.iter()
            .filter(|(_, obj)| self.is_font_dictionary(obj))
            .map(|(id, _)| *id)
            .collect();

        for id in font_objects {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                self.subset_font(dict)?;
            }
        }

        Ok(())
    }

    fn is_font_dictionary(&self, obj: &Object) -> bool {
        if let Object::Dictionary(ref dict) = obj {
            if let Ok(type_name) = dict.get("Type") {
                return matches!(type_name, &Object::Name(ref n) if n == "Font");
            }
        }
        false
    }

    fn subset_font(&self, dict: &mut Dictionary) -> Result<(), PdfError> {
        // Implement font subsetting logic
        // This would involve:
        // 1. Analyzing used characters
        // 2. Creating a subset of the font
        // 3. Updating font dictionary and related structures
        Ok(())
    }

    fn merge_duplicate_resources(&self, doc: &mut Document) -> Result<(), PdfError> {
        if !self.config.merge_duplicate_resources {
            return Ok(());
        }

        let mut resource_map: HashMap<String, ObjectId> = HashMap::new();
        let duplicates: Vec<ObjectId> = Vec::new();

        // Find and merge duplicate resources
        // This is a simplified version; in production, implement proper resource comparison
        for (id, obj) in doc.objects.iter() {
            if let Object::Stream(ref stream) = obj {
                let hash = self.calculate_resource_hash(stream)?;
                if let Some(&existing_id) = resource_map.get(&hash) {
                    duplicates.push(*id);
                    self.update_references(doc, *id, existing_id)?;
                } else {
                    resource_map.insert(hash, *id);
                }
            }
        }

        // Remove duplicate objects
        for id in duplicates {
            doc.objects.remove(&id);
        }

        Ok(())
    }

    fn calculate_resource_hash(&self, stream: &Stream) -> Result<String, PdfError> {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&stream.content);
        Ok(format!("{:x}", hasher.finalize()))
    }

    fn update_references(&self, doc: &mut Document, from: ObjectId, to: ObjectId) -> Result<(), PdfError> {
        // Update all references from the duplicate object to the original
        // This would involve traversing the document and updating all references
        Ok(())
    }

    fn remove_unused_resources(&self, doc: &mut Document) -> Result<(), PdfError> {
        if !self.config.remove_unused_resources {
            return Ok(());
        }

        let used_objects = self.find_used_objects(doc);
        let all_objects: HashSet<ObjectId> = doc.objects.keys().copied().collect();
        
        // Remove unused objects
        for id in all_objects.difference(&used_objects) {
            doc.objects.remove(id);
        }

        Ok(())
    }

    fn find_used_objects(&self, doc: &Document) -> HashSet<ObjectId> {
        let mut used = HashSet::new();
        let mut to_visit = vec![doc.trailer.get("Root").and_then(|r| r.as_reference())];

        while let Some(Some(id)) = to_visit.pop() {
            if used.insert(id) {
                if let Some(obj) = doc.objects.get(&id) {
                    self.collect_references(obj, &mut to_visit);
                }
            }
        }

        used
    }

    fn collect_references(&self, obj: &Object, refs: &mut Vec<Option<ObjectId>>) {
        match obj {
            Object::Reference(id) => refs.push(Some(*id)),
            Object::Array(arr) => {
                for item in arr {
                    self.collect_references(item, refs);
                }
            }
            Object::Dictionary(dict) => {
                for value in dict.values() {
                    self.collect_references(value, refs);
                }
            }
            _ => (),
        }
    }

    fn optimize_structure(&self, doc: &mut Document) -> Result<(), PdfError> {
        // Implement document structure optimization
        // This might include:
        // - Optimizing page tree
        // - Flattening optional content groups
        // - Removing unnecessary levels of indirect references
        Ok(())
    }

    fn get_applied_techniques(&self) -> Vec<OptimizationTechnique> {
        let mut techniques = Vec::new();
        match self.config.level {
            OptimizationLevel::None => (),
            OptimizationLevel::Basic => {
                techniques.push(OptimizationTechnique::ImageCompression);
                techniques.push(OptimizationTechnique::StreamCompression);
            },
            OptimizationLevel::Standard => {
                techniques.push(OptimizationTechnique::ImageCompression);
                techniques.push(OptimizationTechnique::StreamCompression);
                techniques.push(OptimizationTechnique::FontSubsetting);
                techniques.push(OptimizationTechnique::ResourceDeduplification);
            },
            OptimizationLevel::Aggressive => {
                techniques.extend_from_slice(&[
                    OptimizationTechnique::ImageCompression,
                    OptimizationTechnique::StreamCompression,
                    OptimizationTechnique::FontSubsetting,
                    OptimizationTechnique::ResourceDeduplification,
                    OptimizationTechnique::StructureOptimization,
                ]);
            },
        }
        techniques
    }
}

impl Default for OptimizationConfig {
    fn default() -> Self {
        Self {
            level: OptimizationLevel::Standard,
            image_quality: 85,
            max_image_resolution: 2048,
            enable_font_subsetting: true,
            remove_unused_resources: true,
            merge_duplicate_resources: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_optimization_system_creation() {
        let writer_config = WriterConfig::default();
        let metrics = Arc::new(MetricsRegistry::new().unwrap());
        let system = OptimizationSystem::new(&writer_config, metrics).await;
        assert!(system.is_ok());
    }

    #[tokio::test]
    async fn test_document_optimization() {
        let writer_config = WriterConfig::default();
        let metrics = Arc::new(MetricsRegistry::new().unwrap());
        let system = OptimizationSystem::new(&writer_config, metrics).await.unwrap();
        
        let doc = Document::new();
        let result = system.optimize_document(doc).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_aggressive_optimization() {
        let writer_config = WriterConfig::default();
        let metrics = Arc::new(MetricsRegistry::new().unwrap());
        let mut system = OptimizationSystem::new(&writer_config, metrics).await.unwrap();
        
        system.config.level = OptimizationLevel::Aggressive;
        
        let mut doc = Document::new();
        // Add some content to optimize
        doc.objects.insert(
            (1, 0),
            Object::Stream(Stream::new(Dictionary::new(), vec![0u8; 1000])),
        );
        
        let result = system.optimize_document(doc).await;
        assert!(result.is_ok());
    }
}
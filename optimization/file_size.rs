// Auto-patched by Alloma
// Timestamp: 2025-06-02 00:25:08
// User: kartik4091

#![allow(warnings)]

use std::collections::HashMap;
use tokio::sync::RwLock;
use std::sync::Arc;
use crate::core::error::PdfError;

pub struct FileSizeOptimizer {
    config: OptimizerConfig,
    statistics: Arc<RwLock<OptimizationStats>>,
}

#[derive(Debug, Clone)]
pub struct OptimizerConfig {
    pub max_file_size: usize,
    pub compression_level: u32,
    pub image_quality: u32,
    pub optimize_fonts: bool,
    pub remove_metadata: bool,
}

#[derive(Debug, Clone)]
pub struct OptimizationStats {
    pub original_size: usize,
    pub optimized_size: usize,
    pub compression_ratio: f64,
    pub savings_by_type: HashMap<ResourceType, usize>,
    pub processing_time: std::time::Duration,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum ResourceType {
    Images,
    Fonts,
    JavaScript,
    Metadata,
    Other,
}

impl FileSizeOptimizer {
    pub fn new(config: OptimizerConfig) -> Self {
        FileSizeOptimizer {
            config,
            statistics: Arc::new(RwLock::new(OptimizationStats {
                original_size: 0,
                optimized_size: 0,
                compression_ratio: 0.0,
                savings_by_type: HashMap::new(),
                processing_time: std::time::Duration::from_secs(0),
            })),
        }
    }

    pub async fn optimize(&mut self, document: &mut Document) -> Result<OptimizationStats, PdfError> {
        let start_time = std::time::Instant::now();
        let original_size = document.size();

        // Optimize images
        self.optimize_images(document).await?;

        // Optimize fonts
        if self.config.optimize_fonts {
            self.optimize_fonts(document).await?;
        }

        // Remove unnecessary metadata
        if self.config.remove_metadata {
            self.remove_metadata(document).await?;
        }

        // Compress content streams
        self.compress_content(document).await?;

        let end_time = std::time::Instant::now();
        let optimized_size = document.size();

        // Update statistics
        let mut stats = self.statistics.write().await;
        stats.original_size = original_size;
        stats.optimized_size = optimized_size;
        stats.compression_ratio = 1.0 - (optimized_size as f64 / original_size as f64);
        stats.processing_time = end_time - start_time;

        Ok(stats.clone())
    }

    pub async fn get_statistics(&self) -> Result<OptimizationStats, PdfError> {
        Ok(self.statistics.read().await.clone())
    }

    async fn optimize_images(&self, document: &mut Document) -> Result<(), PdfError> {
        let mut savings = 0;

        for image in document.get_images().await? {
            let original_size = image.size();
            
            // Resize large images
            if image.width > 2000 || image.height > 2000 {
                image.resize(2000, 2000, ImageFilter::Lanczos3).await?;
            }

            // Recompress with optimal quality
            image.recompress(self.config.image_quality).await?;

            savings += original_size - image.size();
        }

        let mut stats = self.statistics.write().await;
        stats.savings_by_type.insert(ResourceType::Images, savings);

        Ok(())
    }

    async fn optimize_fonts(&self, document: &mut Document) -> Result<(), PdfError> {
        let mut savings = 0;

        for font in document.get_fonts().await? {
            let original_size = font.size();
            
            // Subset fonts
            font.subset().await?;

            // Compress font data
            font.compress(self.config.compression_level).await?;

            savings += original_size - font.size();
        }

        let mut stats = self.statistics.write().await;
        stats.savings_by_type.insert(ResourceType::Fonts, savings);

        Ok(())
    }

    async fn remove_metadata(&self, document: &mut Document) -> Result<(), PdfError> {
        let original_size = document.get_metadata_size().await?;
        
        // Remove unnecessary metadata
        document.remove_metadata(&[
            "CreationDate",
            "ModDate",
            "Producer",
            "Creator",
        ]).await?;

        let savings = original_size - document.get_metadata_size().await?;

        let mut stats = self.statistics.write().await;
        stats.savings_by_type.insert(ResourceType::Metadata, savings);

        Ok(())
    }

    async fn compress_content(&self, document: &mut Document) -> Result<(), PdfError> {
        let mut savings = 0;

        for stream in document.get_content_streams().await? {
            let original_size = stream.size();
            
            // Optimize stream data
            stream.optimize().await?;
            
            // Compress stream
            stream.compress(self.config.compression_level).await?;

            savings += original_size - stream.size();
        }

        let mut stats = self.statistics.write().await;
        stats.savings_by_type.insert(ResourceType::Other, savings);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_file_size_optimization() {
        let config = OptimizerConfig {
            max_file_size: 10 * 1024 * 1024, // 10MB
            compression_level: 9,
            image_quality: 85,
            optimize_fonts: true,
            remove_metadata: true,
        };

        let mut optimizer = FileSizeOptimizer::new(config);
        let mut document = Document::new(); // Create a test document

        // Add test content
        document.add_image(/* test image data */).await.unwrap();
        document.add_font(/* test font data */).await.unwrap();

        let stats = optimizer.optimize(&mut document).await.unwrap();
        assert!(stats.compression_ratio > 0.0);
    }
}
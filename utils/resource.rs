// Auto-patched by Alloma
// Timestamp: 2025-06-02 00:10:07
// User: kartik4091

#![allow(warnings)]

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::core::error::PdfError;

pub struct ResourceUtils {
    resources: Arc<RwLock<HashMap<String, Resource>>>,
    config: ResourceConfig,
}

#[derive(Debug)]
pub struct Resource {
    resource_type: ResourceType,
    data: Vec<u8>,
    metadata: ResourceMetadata,
}

#[derive(Debug, Clone)]
pub enum ResourceType {
    Font,
    Image,
    ColorSpace,
    Pattern,
    XObject,
    ExtGState,
    Properties,
    Custom(String),
}

#[derive(Debug)]
pub struct ResourceMetadata {
    name: String,
    size: usize,
    created: chrono::DateTime<chrono::Utc>,
    modified: chrono::DateTime<chrono::Utc>,
    attributes: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ResourceConfig {
    pub cache_enabled: bool,
    pub cache_size: usize,
    pub max_resource_size: usize,
}

impl ResourceUtils {
    pub fn new() -> Self {
        ResourceUtils {
            resources: Arc::new(RwLock::new(HashMap::new())),
            config: ResourceConfig {
                cache_enabled: true,
                cache_size: 1000,
                max_resource_size: 10 * 1024 * 1024, // 10MB
            },
        }
    }

    pub async fn add_resource(&mut self, name: String, resource_type: ResourceType, data: Vec<u8>) -> Result<(), PdfError> {
        if data.len() > self.config.max_resource_size {
            return Err(PdfError::InvalidObject("Resource too large".into()));
        }

        let mut resources = self.resources.write().await;
        
        if resources.len() >= self.config.cache_size {
            // Evict oldest resource
            if let Some((k, _)) = resources.iter().next() {
                resources.remove(&k.to_string());
            }
        }

        resources.insert(name.clone(), Resource {
            resource_type,
            data,
            metadata: ResourceMetadata {
                name,
                size: 0,
                created: chrono::Utc::now(),
                modified: chrono::Utc::now(),
                attributes: HashMap::new(),
            },
        });

        Ok(())
    }

    pub async fn get_resource(&self, name: &str) -> Result<Option<Resource>, PdfError> {
        let resources = self.resources.read().await;
        Ok(resources.get(name).cloned())
    }

    pub async fn remove_resource(&mut self, name: &str) -> Result<(), PdfError> {
        let mut resources = self.resources.write().await;
        resources.remove(name);
        Ok(())
    }

    pub async fn list_resources(&self) -> Result<Vec<String>, PdfError> {
        let resources = self.resources.read().await;
        Ok(resources.keys().cloned().collect())
    }
}

impl Clone for Resource {
    fn clone(&self) -> Self {
        Resource {
            resource_type: self.resource_type.clone(),
            data: self.data.clone(),
            metadata: ResourceMetadata {
                name: self.metadata.name.clone(),
                size: self.metadata.size,
                created: self.metadata.created,
                modified: self.metadata.modified,
                attributes: self.metadata.attributes.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resource_management() {
        let mut utils = ResourceUtils::new();
        utils.add_resource(
            "test".to_string(),
            ResourceType::Font,
            vec![1, 2, 3],
        ).await.unwrap();

        let resource = utils.get_resource("test").await.unwrap();
        assert!(resource.is_some());
    }
}
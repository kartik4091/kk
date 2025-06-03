use crate::{PdfError, WriterConfig};
use chrono::{DateTime, Utc};
use lopdf::{Document, Dictionary, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use uuid::Uuid;

pub struct MetadataSystem {
    state: Arc<RwLock<MetadataState>>,
    config: MetadataConfig,
    validator: MetadataValidator,
}

struct MetadataState {
    operations_performed: u64,
    last_operation: Option<DateTime<Utc>>,
    active_operations: u32,
    metadata_cache: HashMap<String, CachedMetadata>,
}

#[derive(Clone)]
struct MetadataConfig {
    enable_xmp: bool,
    pdf_a_compliance: Option<PdfALevel>,
    cache_ttl: std::time::Duration,
    max_cache_size: usize,
    validate_metadata: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    title: Option<String>,
    author: Option<String>,
    subject: Option<String>,
    keywords: Option<String>,
    creator: Option<String>,
    producer: Option<String>,
    creation_date: Option<DateTime<Utc>>,
    modification_date: Option<DateTime<Utc>>,
    trapped: Option<String>,
    custom_properties: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XmpMetadata {
    dublin_core: DublinCore,
    pdf: PdfProperties,
    xmp: XmpProperties,
    custom: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DublinCore {
    title: Option<String>,
    creator: Option<String>,
    description: Option<String>,
    subject: Vec<String>,
    publisher: Option<String>,
    contributor: Option<String>,
    date: Option<DateTime<Utc>>,
    format: Option<String>,
    identifier: Option<String>,
    language: Option<String>,
    rights: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PdfProperties {
    version: String,
    producer: String,
    pdf_a_level: Option<PdfALevel>,
    trapped: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct XmpProperties {
    create_date: DateTime<Utc>,
    modify_date: DateTime<Utc>,
    metadata_date: DateTime<Utc>,
    creator_tool: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PdfALevel {
    A1a,
    A1b,
    A2a,
    A2b,
    A3a,
    A3b,
}

struct CachedMetadata {
    metadata: DocumentMetadata,
    xmp: Option<XmpMetadata>,
    timestamp: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

struct MetadataValidator {
    rules: Vec<Box<dyn Fn(&DocumentMetadata, &Option<XmpMetadata>) -> Result<(), String> + Send + Sync>>,
}

impl MetadataSystem {
    pub async fn new(config: &WriterConfig) -> Result<Self, PdfError> {
        Ok(Self {
            state: Arc::new(RwLock::new(MetadataState {
                operations_performed: 0,
                last_operation: None,
                active_operations: 0,
                metadata_cache: HashMap::new(),
            })),
            config: MetadataConfig::default(),
            validator: MetadataValidator::new(),
        })
    }

    pub async fn update_metadata(
        &self,
        doc: &mut Document,
        metadata: &DocumentMetadata,
    ) -> Result<(), PdfError> {
        let start_time = std::time::Instant::now();
        let current_time = Utc::parse_from_str("2025-06-02 18:43:48", "%Y-%m-%d %H:%M:%S")
            .map_err(|_| PdfError::Processing("Invalid current time".to_string()))?;

        // Update state
        {
            let mut state = self.state.write().map_err(|_| 
                PdfError::Processing("Failed to acquire state lock".to_string()))?;
            state.active_operations += 1;
        }

        // Create info dictionary
        let info_dict = self.create_info_dictionary(metadata, current_time)?;
        let info_id = doc.add_object(info_dict);
        doc.trailer.set("Info", Object::Reference(info_id));

        // Update XMP metadata if enabled
        if self.config.enable_xmp {
            let xmp = self.create_xmp_metadata(metadata, current_time)?;
            self.update_xmp_metadata(doc, &xmp)?;
        }

        // Validate metadata if required
        if self.config.validate_metadata {
            let xmp = if self.config.enable_xmp {
                Some(self.extract_xmp_metadata(doc)?)
            } else {
                None
            };
            self.validator.validate(metadata, &xmp)?;
        }

        // Update state and cache
        {
            let mut state = self.state.write().map_err(|_| 
                PdfError::Processing("Failed to acquire state lock".to_string()))?;
            state.active_operations -= 1;
            state.operations_performed += 1;
            state.last_operation = Some(current_time);

            // Update cache
            state.metadata_cache.insert(
                doc.get_id().unwrap_or_else(|| "unknown".to_string()),
                CachedMetadata {
                    metadata: metadata.clone(),
                    xmp: if self.config.enable_xmp {
                        Some(self.create_xmp_metadata(metadata, current_time)?)
                    } else {
                        None
                    },
                    timestamp: current_time,
                    expires_at: current_time + self.config.cache_ttl,
                },
            );
        }

        Ok(())
    }

    fn create_info_dictionary(
        &self,
        metadata: &DocumentMetadata,
        current_time: DateTime<Utc>,
    ) -> Result<Dictionary, PdfError> {
        let mut dict = Dictionary::new();

        if let Some(title) = &metadata.title {
            dict.set("Title", Object::string(title));
        }
        if let Some(author) = &metadata.author {
            dict.set("Author", Object::string(author));
        }
        if let Some(subject) = &metadata.subject {
            dict.set("Subject", Object::string(subject));
        }
        if let Some(keywords) = &metadata.keywords {
            dict.set("Keywords", Object::string(keywords));
        }

        dict.set("Creator", Object::string(
            metadata.creator.as_deref().unwrap_or("kartik4091")
        ));
        dict.set("Producer", Object::string(
            metadata.producer.as_deref().unwrap_or("PDF Engine 1.0")
        ));
        dict.set("CreationDate", Object::string(
            metadata.creation_date
                .unwrap_or(current_time)
                .to_rfc3339()
        ));
        dict.set("ModDate", Object::string(current_time.to_rfc3339()));

        if let Some(trapped) = &metadata.trapped {
            dict.set("Trapped", Object::string(trapped));
        }

        Ok(dict)
    }

    fn create_xmp_metadata(
        &self,
        metadata: &DocumentMetadata,
        current_time: DateTime<Utc>,
    ) -> Result<XmpMetadata, PdfError> {
        Ok(XmpMetadata {
            dublin_core: DublinCore {
                title: metadata.title.clone(),
                creator: Some("kartik4091".to_string()),
                description: metadata.subject.clone(),
                subject: metadata.keywords
                    .as_ref()
                    .map(|k| k.split(',').map(String::from).collect())
                    .unwrap_or_default(),
                publisher: None,
                contributor: None,
                date: Some(current_time),
                format: Some("application/pdf".to_string()),
                identifier: Some(Uuid::new_v4().to_string()),
                language: Some("en".to_string()),
                rights: None,
            },
            pdf: PdfProperties {
                version: "1.7".to_string(),
                producer: "PDF Engine 1.0".to_string(),
                pdf_a_level: self.config.pdf_a_compliance,
                trapped: None,
            },
            xmp: XmpProperties {
                create_date: metadata.creation_date.unwrap_or(current_time),
                modify_date: current_time,
                metadata_date: current_time,
                creator_tool: "PDF Engine Writer".to_string(),
            },
            custom: metadata.custom_properties.clone(),
        })
    }

    fn update_xmp_metadata(&self, doc: &mut Document, xmp: &XmpMetadata) -> Result<(), PdfError> {
        // Convert XMP metadata to XML
        let xmp_xml = self.serialize_xmp_to_xml(xmp)?;

        // Create metadata stream
        let mut dict = Dictionary::new();
        dict.set("Type", Object::name("Metadata"));
        dict.set("Subtype", Object::name("XML"));
        dict.set("Length", Object::Integer(xmp_xml.len() as i64));

        let metadata_stream = lopdf::Stream::new(dict, xmp_xml.into_bytes());
        let metadata_id = doc.add_object(metadata_stream);

        // Update document catalog
        if let Some(catalog_id) = doc.catalog {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&catalog_id) {
                dict.set("Metadata", Object::Reference(metadata_id));
            }
        }

        Ok(())
    }

    fn serialize_xmp_to_xml(&self, xmp: &XmpMetadata) -> Result<String, PdfError> {
        // This is a simplified XMP XML serialization
        // In production, use proper XMP serialization library
        let mut xml = String::new();
        xml.push_str(r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>"#);
        xml.push_str(r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">"#);
        xml.push_str(r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">"#);
        
        // Add Dublin Core
        xml.push_str(r#"<rdf:Description xmlns:dc="http://purl.org/dc/elements/1.1/">"#);
        if let Some(title) = &xmp.dublin_core.title {
            xml.push_str(&format!(r#"<dc:title>{}</dc:title>"#, escape_xml(title)));
        }
        xml.push_str("</rdf:Description>");

        xml.push_str("</rdf:RDF></x:xmpmeta>");
        xml.push_str(r#"<?xpacket end="w"?>"#);

        Ok(xml)
    }

    fn extract_xmp_metadata(&self, doc: &Document) -> Result<XmpMetadata, PdfError> {
        // This would extract XMP metadata from the document
        // For now, return a default XMP metadata structure
        Ok(XmpMetadata {
            dublin_core: DublinCore::default(),
            pdf: PdfProperties::default(),
            xmp: XmpProperties::default(),
            custom: HashMap::new(),
        })
    }
}

impl MetadataValidator {
    fn new() -> Self {
        Self {
            rules: vec![
                Box::new(|metadata, _| {
                    if metadata.title.as_ref().map_or(0, String::len) > 1000 {
                        return Err("Title too long".to_string());
                    }
                    Ok(())
                }),
                Box::new(|metadata, xmp| {
                    if let (Some(ref info_title), Some(ref xmp)) = (&metadata.title, xmp) {
                        if let Some(ref xmp_title) = xmp.dublin_core.title {
                            if info_title != xmp_title {
                                return Err("Title mismatch between Info and XMP".to_string());
                            }
                        }
                    }
                    Ok(())
                }),
            ],
        }
    }

    fn validate(
        &self,
        metadata: &DocumentMetadata,
        xmp: &Option<XmpMetadata>,
    ) -> Result<(), PdfError> {
        for rule in &self.rules {
            if let Err(msg) = rule(metadata, xmp) {
                return Err(PdfError::Processing(format!("Metadata validation failed: {}", msg)));
            }
        }
        Ok(())
    }
}

impl Default for MetadataConfig {
    fn default() -> Self {
        Self {
            enable_xmp: true,
            pdf_a_compliance: Some(PdfALevel::A2b),
            cache_ttl: std::time::Duration::from_secs(300), // 5 minutes
            max_cache_size: 1000,
            validate_metadata: true,
        }
    }
}

impl Default for DocumentMetadata {
    fn default() -> Self {
        Self {
            title: None,
            author: Some("kartik4091".to_string()),
            subject: None,
            keywords: None,
            creator: Some("PDF Engine".to_string()),
            producer: Some("PDF Engine 1.0".to_string()),
            creation_date: Some(Utc::now()),
            modification_date: Some(Utc::now()),
            trapped: None,
            custom_properties: HashMap::new(),
        }
    }
}

impl Default for DublinCore {
    fn default() -> Self {
        Self {
            title: None,
            creator: Some("kartik4091".to_string()),
            description: None,
            subject: Vec::new(),
            publisher: None,
            contributor: None,
            date: Some(Utc::now()),
            format: Some("application/pdf".to_string()),
            identifier: Some(Uuid::new_v4().to_string()),
            language: Some("en".to_string()),
            rights: None,
        }
    }
}

impl Default for PdfProperties {
    fn default() -> Self {
        Self {
            version: "1.7".to_string(),
            producer: "PDF Engine 1.0".to_string(),
            pdf_a_level: Some(PdfALevel::A2b),
            trapped: None,
        }
    }
}

impl Default for XmpProperties {
    fn default() -> Self {
        Self {
            create_date: Utc::now(),
            modify_date: Utc::now(),
            metadata_date: Utc::now(),
            creator_tool: "PDF Engine Writer".to_string(),
        }
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metadata_system_creation() {
        let config = WriterConfig::default();
        let system = MetadataSystem::new(&config).await;
        assert!(system.is_ok());
    }

    #[tokio::test]
    async fn test_metadata_update() {
        let config = WriterConfig::default();
        let system = MetadataSystem::new(&config).await.unwrap();
        
        let mut doc = Document::new();
        let metadata = DocumentMetadata {
            title: Some("Test Document".to_string()),
            author: Some("kartik4091".to_string()),
            ..Default::default()
        };
        
        let result = system.update_metadata(&mut doc, &metadata).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_xmp_serialization() {
        let config = WriterConfig::default();
        let system = MetadataSystem::new(&config).await.unwrap();
        
        let metadata = DocumentMetadata::default();
        let current_time = Utc::parse_from_str("2025-06-02 18:43:48", "%Y-%m-%d %H:%M:%S").unwrap();
        
        let xmp = system.create_xmp_metadata(&metadata, current_time).unwrap();
        let xml = system.serialize_xmp_to_xml(&xmp);
        assert!(xml.is_ok());
    }
}
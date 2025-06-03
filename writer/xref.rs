use crate::{PdfError, WriterConfig};
use chrono::{DateTime, Utc};
use lopdf::{Document, ObjectId, Object, Dictionary, Stream};
use std::{
    collections::{HashMap, BTreeMap},
    sync::{Arc, RwLock},
};

pub struct XrefSystem {
    state: Arc<RwLock<XrefState>>,
    config: XrefConfig,
    cache: Arc<RwLock<XrefCache>>,
}

struct XrefState {
    operations_performed: u64,
    last_operation: Option<DateTime<Utc>>,
    active_operations: u32,
    xref_stats: HashMap<String, XrefStats>,
}

#[derive(Clone)]
struct XrefConfig {
    enable_compression: bool,
    enable_caching: bool,
    cache_ttl: std::time::Duration,
    max_cache_size: usize,
    use_stream_xref: bool,
}

#[derive(Debug)]
struct XrefStats {
    entries_count: usize,
    compressed_size: Option<usize>,
    timestamp: DateTime<Utc>,
    is_stream: bool,
}

#[derive(Debug)]
struct XrefEntry {
    offset: u64,
    generation: u16,
    in_use: bool,
}

struct XrefCache {
    entries: HashMap<String, CachedXref>,
    size: usize,
}

struct CachedXref {
    table: BTreeMap<ObjectId, XrefEntry>,
    timestamp: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl XrefSystem {
    pub async fn new(config: &WriterConfig) -> Result<Self, PdfError> {
        Ok(Self {
            state: Arc::new(RwLock::new(XrefState {
                operations_performed: 0,
                last_operation: None,
                active_operations: 0,
                xref_stats: HashMap::new(),
            })),
            config: XrefConfig::default(),
            cache: Arc::new(RwLock::new(XrefCache {
                entries: HashMap::new(),
                size: 0,
            })),
        })
    }

    pub async fn write_xref(
        &self,
        doc: &mut Document,
        objects: &BTreeMap<ObjectId, Object>,
    ) -> Result<u64, PdfError> {
        let start_time = std::time::Instant::now();
        let current_time = Utc::parse_from_str("2025-06-02 18:51:35", "%Y-%m-%d %H:%M:%S")
            .map_err(|_| PdfError::Processing("Invalid current time".to_string()))?;

        // Update state
        {
            let mut state = self.state.write().map_err(|_| 
                PdfError::Processing("Failed to acquire state lock".to_string()))?;
            state.active_operations += 1;
        }

        let xref_offset = if self.config.use_stream_xref {
            self.write_xref_stream(doc, objects, current_time)?
        } else {
            self.write_xref_table(doc, objects, current_time)?
        };

        // Update state
        {
            let mut state = self.state.write().map_err(|_| 
                PdfError::Processing("Failed to acquire state lock".to_string()))?;
            state.active_operations -= 1;
            state.operations_performed += 1;
            state.last_operation = Some(current_time);
        }

        Ok(xref_offset)
    }

    fn write_xref_table(
        &self,
        doc: &mut Document,
        objects: &BTreeMap<ObjectId, Object>,
        current_time: DateTime<Utc>,
    ) -> Result<u64, PdfError> {
        let mut xref_entries = BTreeMap::new();
        let mut offset = 0u64;

        // Add null object as first entry
        xref_entries.insert(
            (0, 0),
            XrefEntry {
                offset: 0,
                generation: 65535,
                in_use: false,
            },
        );

        // Build xref entries
        for (&id, obj) in objects {
            let entry = XrefEntry {
                offset,
                generation: id.1,
                in_use: true,
            };
            xref_entries.insert(id, entry);

            // Update offset for next object
            offset += self.calculate_object_size(obj)?;
        }

        // Write xref table
        let xref_offset = offset;
        let mut xref_data = Vec::new();

        // Write xref header
        xref_data.extend(b"xref\n");

        // Write xref sections
        let mut current_section = Vec::new();
        let mut last_id = None;

        for (&id, entry) in &xref_entries {
            match last_id {
                None => {
                    current_section.push((id, entry));
                }
                Some(last) => {
                    if id.0 as u32 != last.0 as u32 + 1 {
                        // Write current section
                        self.write_xref_section(&mut xref_data, &current_section)?;
                        current_section.clear();
                    }
                    current_section.push((id, entry));
                }
            }
            last_id = Some(id);
        }

        // Write last section
        if !current_section.is_empty() {
            self.write_xref_section(&mut xref_data, &current_section)?;
        }

        // Update cache if enabled
        if self.config.enable_caching {
            self.update_cache(&doc.get_id().unwrap_or_default(), xref_entries, current_time)?;
        }

        // Update stats
        self.update_stats(
            doc.get_id().unwrap_or_default(),
            xref_data.len(),
            None,
            current_time,
            false,
        )?;

        Ok(xref_offset)
    }

    fn write_xref_stream(
        &self,
        doc: &mut Document,
        objects: &BTreeMap<ObjectId, Object>,
        current_time: DateTime<Utc>,
    ) -> Result<u64, PdfError> {
        let mut xref_entries = BTreeMap::new();
        let mut offset = 0u64;

        // Build xref entries similar to table
        for (&id, obj) in objects {
            let entry = XrefEntry {
                offset,
                generation: id.1,
                in_use: true,
            };
            xref_entries.insert(id, entry);
            offset += self.calculate_object_size(obj)?;
        }

        // Create xref stream
        let xref_offset = offset;
        let mut stream_data = Vec::new();

        // Write entries in compressed format
        for entry in xref_entries.values() {
            stream_data.extend_from_slice(&entry.offset.to_be_bytes());
            stream_data.push(entry.generation as u8);
            stream_data.push(if entry.in_use { 1 } else { 0 });
        }

        // Create stream dictionary
        let mut dict = Dictionary::new();
        dict.set("Type", Object::Name("XRef".to_string()));
        dict.set("Size", Object::Integer(objects.len() as i64 + 1));
        dict.set("W", Object::Array(vec![
            Object::Integer(8), // Offset field size
            Object::Integer(1), // Generation field size
            Object::Integer(1), // In-use flag field size
        ]));

        // Create and add xref stream
        let xref_stream = Stream::new(dict, stream_data);
        let xref_stream_id = doc.add_object(xref_stream);
        doc.trailer.set("XRefStm", Object::Integer(xref_offset as i64));

        // Update cache if enabled
        if self.config.enable_caching {
            self.update_cache(&doc.get_id().unwrap_or_default(), xref_entries, current_time)?;
        }

        // Update stats
        self.update_stats(
            doc.get_id().unwrap_or_default(),
            0,
            Some(stream_data.len()),
            current_time,
            true,
        )?;

        Ok(xref_offset)
    }

    fn write_xref_section(
        &self,
        output: &mut Vec<u8>,
        section: &[(ObjectId, &XrefEntry)],
    ) -> Result<(), PdfError> {
        if section.is_empty() {
            return Ok(());
        }

        // Write section header
        let start_obj = section[0].0.0;
        let count = section.len();
        output.extend(format!("{} {}\n", start_obj, count).as_bytes());

        // Write entries
        for (_, entry) in section {
            output.extend(
                format!(
                    "{:010} {:05} {}\n",
                    entry.offset,
                    entry.generation,
                    if entry.in_use { "n" } else { "f" }
                )
                .as_bytes(),
            );
        }

        Ok(())
    }

    fn calculate_object_size(&self, obj: &Object) -> Result<u64, PdfError> {
        // In a real implementation, this would calculate the exact size
        // For now, return a reasonable estimate
        Ok(match obj {
            Object::Stream(stream) => stream.content.len() as u64 + 100, // Add overhead
            Object::Dictionary(dict) => dict.len() as u64 * 50, // Estimate
            Object::Array(arr) => arr.len() as u64 * 20,       // Estimate
            Object::String(s) => s.len() as u64 + 2,           // Add delimiters
            Object::Integer(_) => 20,                          // Max int length
            Object::Real(_) => 20,                            // Max real length
            Object::Boolean(_) => 5,                          // "true" or "false"
            Object::Name(n) => n.len() as u64 + 1,           // Add '/' prefix
            Object::Reference(_) => 15,                       // "x x R" format
            Object::Null => 4,                               // "null"
        })
    }

    fn update_cache(
        &self,
        doc_id: &str,
        entries: BTreeMap<ObjectId, XrefEntry>,
        current_time: DateTime<Utc>,
    ) -> Result<(), PdfError> {
        if !self.config.enable_caching {
            return Ok(());
        }

        let mut cache = self.cache.write().map_err(|_| 
            PdfError::Processing("Failed to acquire cache lock".to_string()))?;

        // Ensure cache size limit
        while cache.size > self.config.max_cache_size {
            if let Some((key, _)) = cache.entries.iter()
                .min_by_key(|(_, entry)| entry.timestamp) {
                let key = key.clone();
                cache.entries.remove(&key);
                cache.size -= 1;
            } else {
                break;
            }
        }

        cache.entries.insert(doc_id.to_string(), CachedXref {
            table: entries,
            timestamp: current_time,
            expires_at: current_time + self.config.cache_ttl,
        });
        cache.size += 1;

        Ok(())
    }

    fn update_stats(
        &self,
        doc_id: String,
        table_size: usize,
        stream_size: Option<usize>,
        current_time: DateTime<Utc>,
        is_stream: bool,
    ) -> Result<(), PdfError> {
        let mut state = self.state.write().map_err(|_| 
            PdfError::Processing("Failed to acquire state lock".to_string()))?;

        state.xref_stats.insert(doc_id, XrefStats {
            entries_count: if is_stream {
                stream_size.unwrap_or(0) / 10 // Approximate entry count from stream size
            } else {
                table_size / 20 // Approximate entry count from table size
            },
            compressed_size: stream_size,
            timestamp: current_time,
            is_stream,
        });

        Ok(())
    }
}

impl Default for XrefConfig {
    fn default() -> Self {
        Self {
            enable_compression: true,
            enable_caching: true,
            cache_ttl: std::time::Duration::from_secs(300), // 5 minutes
            max_cache_size: 1000, // Maximum number of cached xref tables
            use_stream_xref: true, // Use compressed xref streams by default
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_xref_system_creation() {
        let config = WriterConfig::default();
        let system = XrefSystem::new(&config).await;
        assert!(system.is_ok());
    }

    #[tokio::test]
    async fn test_xref_table_writing() {
        let config = WriterConfig::default();
        let system = XrefSystem::new(&config).await.unwrap();
        
        let mut doc = Document::new();
        let mut objects = BTreeMap::new();
        objects.insert((1, 0), Object::String("Test".to_string()));
        
        let result = system.write_xref(&mut doc, &objects).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_xref_stream_writing() {
        let config = WriterConfig::default();
        let system = XrefSystem::new(&config).await.unwrap();
        
        let mut doc = Document::new();
        let mut objects = BTreeMap::new();
        objects.insert((1, 0), Object::String("Test".to_string()));
        
        system.config.use_stream_xref = true;
        let result = system.write_xref(&mut doc, &objects).await;
        assert!(result.is_ok());
    }
}
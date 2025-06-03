//! Structure cleaning implementation for PDF anti-forensics
//! Created: 2025-06-03 14:26:59 UTC
//! Author: kartik4091

use std::collections::{HashMap, HashSet};
use tracing::{debug, error, info, instrument, warn};

use crate::{
    error::{Error, Result},
    types::{Document, Object, ObjectId, XRefEntry, XRefTable},
};

/// Structure cleaner for PDF documents
pub struct StructureCleaner {
    /// Cleaning statistics
    stats: CleaningStats,
    
    /// Object references
    references: HashMap<ObjectId, HashSet<ObjectId>>,
    
    /// Removed objects
    removed_objects: HashSet<ObjectId>,
}

/// Structure cleaning statistics
#[derive(Debug, Default)]
pub struct CleaningStats {
    /// Number of objects removed
    pub objects_removed: usize,
    
    /// Number of references updated
    pub references_updated: usize,
    
    /// Number of cross-references updated
    pub xrefs_updated: usize,
    
    /// Number of structure optimizations
    pub optimizations: usize,
    
    /// Processing duration in milliseconds
    pub duration_ms: u64,
}

/// Structure cleaning configuration
#[derive(Debug, Clone)]
pub struct CleaningConfig {
    /// Remove unreferenced objects
    pub remove_unreferenced: bool,
    
    /// Optimize object structure
    pub optimize_structure: bool,
    
    /// Compact object numbers
    pub compact_numbers: bool,
    
    /// Update cross-references
    pub update_xrefs: bool,
}

impl Default for CleaningConfig {
    fn default() -> Self {
        Self {
            remove_unreferenced: true,
            optimize_structure: true,
            compact_numbers: true,
            update_xrefs: true,
        }
    }
}

impl StructureCleaner {
    /// Create a new structure cleaner
    pub fn new() -> Self {
        Self {
            stats: CleaningStats::default(),
            references: HashMap::new(),
            removed_objects: HashSet::new(),
        }
    }
    
    /// Clean document structure
    #[instrument(skip(self, document))]
    pub async fn clean_document(&mut self, document: Document) -> Result<(Document, usize)> {
        let start_time = std::time::Instant::now();
        info!("Starting structure cleaning");
        
        let mut cleaned_doc = document;
        
        // Build reference map
        self.build_reference_map(&cleaned_doc);
        
        // Remove unreferenced objects
        self.remove_unreferenced_objects(&mut cleaned_doc)?;
        
        // Optimize object structure
        self.optimize_structure(&mut cleaned_doc)?;
        
        // Update cross-references
        self.update_cross_references(&mut cleaned_doc)?;
        
        // Compact object numbers
        self.compact_object_numbers(&mut cleaned_doc)?;
        
        // Update statistics
        self.stats.duration_ms = start_time.elapsed().as_millis() as u64;
        let total_changes = self.stats.objects_removed + 
                          self.stats.references_updated +
                          self.stats
                          .optimizations;
                          
        info!("Structure cleaning completed with {} changes", total_changes);
        Ok((cleaned_doc, total_changes))
    }
    
    /// Build object reference map
    fn build_reference_map(&mut self, document: &Document) {
        self.references.clear();
        
        for (&object_id, object) in &document.structure.objects {
            let mut refs = HashSet::new();
            self.collect_references(object, &mut refs);
            if !refs.is_empty() {
                self.references.insert(object_id, refs);
            }
        }
    }
    
    /// Collect object references
    fn collect_references(&self, object: &Object, refs: &mut HashSet<ObjectId>) {
        match object {
            Object::Reference(id) => {
                refs.insert(*id);
            }
            Object::Array(array) => {
                for item in array {
                    self.collect_references(item, refs);
                }
            }
            Object::Dictionary(dict) => {
                for value in dict.values() {
                    self.collect_references(value, refs);
                }
            }
            Object::Stream { dict, .. } => {
                for value in dict.values() {
                    self.collect_references(value, refs);
                }
            }
            _ => {}
        }
    }
    
    /// Remove unreferenced objects
    fn remove_unreferenced_objects(&mut self, document: &mut Document) -> Result<()> {
        let mut referenced = HashSet::new();
        
        // Add root and essential objects
        referenced.insert(document.structure.trailer.root);
        if let Some(info) = document.structure.trailer.info {
            referenced.insert(info);
        }
        
        // Add objects referenced from root
        let mut to_process = vec![document.structure.trailer.root];
        while let Some(id) = to_process.pop() {
            if let Some(refs) = self.references.get(&id) {
                for &ref_id in refs {
                    if referenced.insert(ref_id) {
                        to_process.push(ref_id);
                    }
                }
            }
        }
        
        // Remove unreferenced objects
        let mut removed = 0;
        document.structure.objects.retain(|&id, _| {
            let keep = referenced.contains(&id);
            if !keep {
                removed += 1;
                self.removed_objects.insert(id);
            }
            keep
        });
        
        self.stats.objects_removed += removed;
        Ok(())
    }
    
    /// Optimize object structure
    fn optimize_structure(&mut self, document: &mut Document) -> Result<()> {
        let mut optimizations = 0;
        
        // Merge small streams
        optimizations += self.merge_small_streams(document)?;
        
        // Combine similar objects
        optimizations += self.combine_similar_objects(document)?;
        
        self.stats.optimizations += optimizations;
        Ok(())
    }
    
    /// Merge small streams
    fn merge_small_streams(&mut self, document: &mut Document) -> Result<usize> {
        let mut merged = 0;
        let mut small_streams = Vec::new();
        
        // Collect small streams
        for (&id, object) in &document.structure.objects {
            if let Object::Stream { dict, data } = object {
                if data.len() < 1024 {  // Threshold for small streams
                    small_streams.push(id);
                }
            }
        }
        
        // Merge consecutive small streams
        for window in small_streams.windows(2) {
            if let [id1, id2] = window {
                if let (Some(Object::Stream { dict: dict1, data: data1 }), 
                        Some(Object::Stream { dict: dict2, data: data2 })) = 
                    (document.structure.objects.get(id1), document.structure.objects.get(id2)) {
                    if self.can_merge_streams(dict1, dict2) {
                        let merged_data = [data1.clone(), data2.clone()].concat();
                        let mut merged_dict = dict1.clone();
                        merged_dict.insert(b"Length".to_vec(), Object::Integer(merged_data.len() as i64));
                        
                        document.structure.objects.insert(*id1, Object::Stream {
                            dict: merged_dict,
                            data: merged_data,
                        });
                        document.structure.objects.remove(id2);
                        merged += 1;
                    }
                }
            }
        }
        
        Ok(merged)
    }
    
    /// Check if streams can be merged
    fn can_merge_streams(&self, dict1: &HashMap<Vec<u8>, Object>, dict2: &HashMap<Vec<u8>, Object>) -> bool {
        // Check if streams have compatible filters and types
        let filter1 = dict1.get(b"Filter");
        let filter2 = dict2.get(b"Filter");
        let type1 = dict1.get(b"Type");
        let type2 = dict2.get(b"Type");
        
        filter1 == filter2 && type1 == type2
    }
    
    /// Combine similar objects
    fn combine_similar_objects(&mut self, document: &mut Document) -> Result<usize> {
        let mut combined = 0;
        let mut object_map = HashMap::new();
        
        // Group similar objects
        for (&id, object) in &document.structure.objects {
            let hash = self.calculate_object_hash(object);
            object_map.entry(hash).or_insert_with(Vec::new).push(id);
        }
        
        // Combine objects with same hash
        for ids in object_map.values() {
            if ids.len() > 1 {
                let &primary = ids.first().unwrap();
                for &duplicate in &ids[1..] {
                    self.update_references(document, duplicate, primary)?;
                    document.structure.objects.remove(&duplicate);
                    combined += 1;
                }
            }
        }
        
        Ok(combined)
    }
    
    /// Calculate object hash for comparison
    fn calculate_object_hash(&self, object: &Object) -> u64 {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        
        let mut hasher = DefaultHasher::new();
        match object {
            Object::Stream { dict, data } => {
                // Hash stream type and filter
                if let Some(type_obj) = dict.get(b"Type") {
                    type_obj.hash(&mut hasher);
                }
                if let Some(filter) = dict.get(b"Filter") {
                    filter.hash(&mut hasher);
                }
                // Hash first 1KB of data
                data.iter().take(1024).for_each(|b| b.hash(&mut hasher));
            }
            _ => {
                object.hash(&mut hasher);
            }
        }
        hasher.finish()
    }
    
    /// Update references to combined objects
    fn update_references(&mut self, document: &mut Document, from: ObjectId, to: ObjectId) -> Result<()> {
        for object in document.structure.objects.values_mut() {
            self.replace_reference(object, from, to);
        }
        self.stats.references_updated += 1;
        Ok(())
    }
    
    /// Replace object reference
    fn replace_reference(&self, object: &mut Object, from: ObjectId, to: ObjectId) {
        match object {
            Object::Reference(id) if *id == from => {
                *id = to;
            }
            Object::Array(array) => {
                for item in array {
                    self.replace_reference(item, from, to);
                }
            }
            Object::Dictionary(dict) => {
                for value in dict.values_mut() {
                    self.replace_reference(value, from, to);
                }
            }
            Object::Stream { dict, .. } => {
                for value in dict.values_mut() {
                    self.replace_reference(value, from, to);
                }
            }
            _ => {}
        }
    }
    
    /// Update cross-references
    fn update_cross_references(&mut self, document: &mut Document) -> Result<()> {
        let mut updated = 0;
        let mut new_xref = XRefTable {
            offset: 0,
            entries: Vec::new(),
            compressed: false,
        };
        
        let mut offset = 0;
        for (&id, object) in &document.structure.objects {
            if !self.removed_objects.contains(&id) {
                new_xref.entries.push(XRefEntry {
                    object_id: id,
                    offset,
                    generation: 0,
                    entry_type: crate::types::XRefEntryType::InUse,
                });
                offset += self.calculate_object_size(object);
                updated += 1;
            }
        }
        
        document.structure.xref_tables = vec![new_xref];
        self.stats.xrefs_updated += updated;
        Ok(())
    }
    
    /// Calculate object size
    fn calculate_object_size(&self, object: &Object) -> u64 {
        match object {
            Object::Stream { dict, data } => {
                // Rough estimation: dictionary size + stream data size
                let dict_size = dict.iter()
                    .map(|(k, v)| k.len() + self.calculate_object_size(v))
                    .sum::<u64>();
                dict_size + data.len() as u64
            }
            Object::Dictionary(dict) => {
                dict.iter()
                    .map(|(k, v)| k.len() + self.calculate_object_size(v))
                    .sum()
            }
            Object::Array(array) => {
                array.iter()
                    .map(|obj| self.calculate_object_size(obj))
                    .sum()
            }
            Object::String(s) | Object::Name(s) => s.len() as u64,
            Object::Integer(_) | Object::Real(_) => 8,
            Object::Reference(_) => 16,
            Object::Boolean(_) => 1,
            Object::Null => 4,
        }
    }
    
    /// Compact object numbers
    fn compact_object_numbers(&mut self, document: &mut Document) -> Result<()> {
        let mut number_map = HashMap::new();
        let mut next_number = 1u32;
        
        // Create mapping of old to new numbers
        for &id in document.structure.objects.keys() {
            if !self.removed_objects.contains(&id) {
                number_map.insert(id, ObjectId {
                    number: next_number,
                    generation: 0,
                });
                next_number += 1;
            }
        }
        
        // Update object references
        let mut new_objects = HashMap::new();
        for (old_id, object) in document.structure.objects.drain() {
            if let Some(&new_id) = number_map.get(&old_id) {
                let mut new_object = object;
                self.update_object_references(&mut new_object, &number_map);
                new_objects.insert(new_id, new_object);
            }
        }
        
        document.structure.objects = new_objects;
        
        // Update trailer
        if let Some(&new_root) = number_map.get(&document.structure.trailer.root) {
            document.structure.trailer.root = new_root;
        }
        if let Some(info) = document.structure.trailer.info {
            if let Some(&new_info) = number_map.get(&info) {
                document.structure.trailer.info = Some(new_info);
            }
        }
        
        Ok(())
    }
    
    /// Update object references after compaction
    fn update_object_references(&self, object: &mut Object, number_map: &HashMap<ObjectId, ObjectId>) {
        match object {
            Object::Reference(id) => {
                if let Some(&new_id) = number_map.get(id) {
                    *id = new_id;
                }
            }
            Object::Array(array) => {
                for item in array {
                    self.update_object_references(item, number_map);
                }
            }
            Object::Dictionary(dict) => {
                for value in dict.values_mut() {
                    self.update_object_references(value, number_map);
                }
            }
            Object::Stream { dict, .. } => {
                for value in dict.values_mut() {
                    self.update_object_references(value, number_map);
                }
            }
            _ => {}
        }
    }
    
    /// Get cleaning statistics
    pub fn statistics(&self) -> &CleaningStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_build_reference_map() {
        let mut cleaner = StructureCleaner::new();
        let mut document = Document::default();
        
        let id1 = ObjectId { number: 1, generation: 0 };
        let id2 = ObjectId { number: 2, generation: 0 };
        
        document.structure.objects.insert(id1, Object::Reference(id2));
        
        cleaner.build_reference_map(&document);
        
        assert!(cleaner.references.contains_key(&id1));
        assert!(cleaner.references[&id1].contains(&id2));
    }
    
    #[test]
    fn test_remove_unreferenced_objects() {
        let mut cleaner = StructureCleaner::new();
        let mut document = Document::default();
        
        let root = ObjectId { number: 1, generation: 0 };
        let referenced = ObjectId { number: 2, generation: 0 };
        let unreferenced = ObjectId { number: 3, generation: 0 };
        
        document.structure.trailer.root = root;
        document.structure.objects.insert(root, Object::Reference(referenced));
        document.structure.objects.insert(referenced, Object::Null);
        document.structure.objects.insert(unreferenced, Object::Null);
        
        cleaner.remove_unreferenced_objects(&mut document).unwrap();
        
        assert!(document.structure.objects.contains_key(&root));
        assert!(document.structure.objects.contains_key(&referenced));
        assert!(!document.structure.objects.contains_key(&unreferenced));
    }
    
    #[test]
    fn test_merge_small_streams() {
        let mut cleaner = StructureCleaner::new();
        let mut document = Document::default();
        
        let id1 = ObjectId { number: 1, generation: 0 };
        let id2 = ObjectId { number: 2, generation: 0 };
        
        let mut dict1 = HashMap::new();
        dict1.insert(b"Filter".to_vec(), Object::Name(b"FlateDecode".to_vec()));
        
        let mut dict2 = HashMap::new();
        dict2.insert(b"Filter".to_vec(), Object::Name(b"FlateDecode".to_vec()));
        
        document.structure.objects.insert(id1, Object::Stream {
            dict: dict1,
            data: vec![1, 2, 3],
        });
        
        document.structure.objects.insert(id2, Object::Stream {
            dict: dict2,
            data: vec![4, 5, 6],
        });
        
        cleaner.merge_small_streams(&mut document).unwrap();
        
        assert!(document.structure.objects.len() < 2);
    }
}

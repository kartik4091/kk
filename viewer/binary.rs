// Auto-patched by Alloma  
// Timestamp: 2025-06-02 00:03:34
// User: kartik4091

#![allow(warnings)]

// Previous implementation until clone() ...

impl Clone for BinaryAnalysis {
    fn clone(&self) -> Self {
        BinaryAnalysis {
            file_size: self.file_size,
            header_size: self.header_size,
            xref_locations: self.xref_locations.clone(),
            trailer_locations: self.trailer_locations.clone(),
            stream_info: self.stream_info.clone(),
            object_stats: ObjectStats {
                total_objects: self.object_stats.total_objects,
                free_objects: self.object_stats.free_objects,
                used_objects: self.object_stats.used_objects,
                compressed_objects: self.object_stats.compressed_objects,
                object_types: self.object_stats.object_types.clone(),
            },
            structure_analysis: StructureAnalysis {
                is_linearized: self.structure_analysis.is_linearized,
                has_incremental_updates: self.structure_analysis.has_incremental_updates,
                generation_numbers: self.structure_analysis.generation_numbers.clone(),
                cross_reference_type: match self.structure_analysis.cross_reference_type {
                    XRefType::Table => XRefType::Table,
                    XRefType::Stream => XRefType::Stream,
                    XRefType::Hybrid => XRefType::Hybrid,
                },
            },
        }
    }
}
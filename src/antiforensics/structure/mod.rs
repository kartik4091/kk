//! Structure analysis module for PDF anti-forensics
//! Created: 2025-06-03 14:05:03 UTC
//! Author: kartik4091

mod structure_handler;
mod parser;
mod cross_ref;
mod linearization;

pub use self::{
    structure_handler::StructureHandler,
    parser::PDFParser,
    cross_ref::CrossRefHandler,
    linearization::LinearizationHandler,
};

use std::collections::HashMap;
use crate::{
    error::{Error, Result},
    types::{Document, Object, ObjectId},
};

/// Structure analysis result
#[derive(Debug)]
pub struct StructureAnalysis {
    /// Document structure issues found
    pub issues: Vec<StructureIssue>,
    
    /// Object relationships
    pub relationships: ObjectRelationships,
    
    /// Document metrics
    pub metrics: DocumentMetrics,
    
    /// Analysis statistics
    pub statistics: AnalysisStatistics,
}

/// Structure issue severity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueSeverity {
    /// Minor issue
    Minor,
    /// Moderate issue
    Moderate,
    /// Major issue
    Major,
    /// Critical issue
    Critical,
}

/// Structure issue details
#[derive(Debug)]
pub struct StructureIssue {
    /// Issue severity
    pub severity: IssueSeverity,
    
    /// Issue description
    pub description: String,
    
    /// Affected object ID
    pub object_id: Option<ObjectId>,
    
    /// Issue location
    pub location: IssueLocation,
    
    /// Issue context
    pub context: String,
    
    /// Recommended action
    pub recommendation: String,
}

/// Issue location in document
#[derive(Debug, Clone)]
pub enum IssueLocation {
    /// Header section
    Header,
    
    /// Cross-reference table
    CrossRef {
        /// Table offset
        offset: u64,
    },
    
    /// Object stream
    ObjectStream {
        /// Stream object ID
        stream_id: ObjectId,
        /// Offset in stream
        offset: Option<u64>,
    },
    
    /// Trailer
    Trailer,
    
    /// Other location
    Other(String),
}

/// Object relationship graph
#[derive(Debug, Default)]
pub struct ObjectRelationships {
    /// Direct references between objects
    pub references: HashMap<ObjectId, Vec<ObjectId>>,
    
    /// Indirect references between objects
    pub indirect_refs: HashMap<ObjectId, Vec<ObjectId>>,
    
    /// Parent-child relationships
    pub hierarchy: HashMap<ObjectId, Vec<ObjectId>>,
    
    /// Shared objects (referenced by multiple objects)
    pub shared: Vec<ObjectId>,
}

/// Document structure metrics
#[derive(Debug, Default)]
pub struct DocumentMetrics {
    /// Total number of objects
    pub object_count: usize,
    
    /// Number of free objects
    pub free_objects: usize,
    
    /// Number of compressed objects
    pub compressed_objects: usize,
    
    /// Cross-reference table size
    pub xref_size: usize,
    
    /// Maximum object number
    pub max_object_number: u32,
    
    /// Maximum generation number
    pub max_generation: u16,
    
    /// Document linearization status
    pub is_linearized: bool,
    
    /// Document structure depth
    pub structure_depth: usize,
}

/// Analysis statistics
#[derive(Debug, Default)]
pub struct AnalysisStatistics {
    /// Number of objects analyzed
    pub objects_analyzed: usize,
    
    /// Number of streams processed
    pub streams_processed: usize,
    
    /// Number of references validated
    pub references_validated: usize,
    
    /// Number of issues found
    pub issues_found: usize,
    
    /// Analysis duration in milliseconds
    pub duration_ms: u64,
}

/// Structure analysis configuration
#[derive(Debug, Clone)]
pub struct AnalysisConfig {
    /// Maximum depth for recursive analysis
    pub max_depth: usize,
    
    /// Enable reference validation
    pub validate_references: bool,
    
    /// Enable object stream validation
    pub validate_streams: bool,
    
    /// Enable linearization check
    pub check_linearization: bool,
    
    /// Enable structure optimization suggestions
    pub suggest_optimizations: bool,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            max_depth: 100,
            validate_references: true,
            validate_streams: true,
            check_linearization: true,
            suggest_optimizations: true,
        }
    }
}

/// Analysis progress callback
pub type ProgressCallback = Box<dyn Fn(ProgressUpdate) + Send + Sync>;

/// Analysis progress update
#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    /// Current stage
    pub stage: AnalysisStage,
    
    /// Progress percentage (0-100)
    pub progress: f32,
    
    /// Current operation description
    pub operation: String,
    
    /// Objects processed so far
    pub objects_processed: usize,
    
    /// Issues found so far
    pub issues_found: usize,
}

/// Analysis stages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisStage {
    /// Initial setup
    Setup,
    
    /// Cross-reference analysis
    CrossRef,
    
    /// Object analysis
    Objects,
    
    /// Stream analysis
    Streams,
    
    /// Reference validation
    References,
    
    /// Linearization check
    Linearization,
    
    /// Final validation
    Validation,
    
    /// Completed
    Complete,
}

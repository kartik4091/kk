//! Pattern analysis module for anti-forensics
//! Created: 2025-06-03 13:53:23 UTC
//! Author: kartik4091

mod matcher;
mod database;

pub use self::{
    matcher::PatternMatcher,
    database::PatternDatabase,
};

use crate::{
    error::Result,
    types::Document,
};

/// Pattern analysis configuration
#[derive(Debug, Clone)]
pub struct PatternConfig {
    /// Enable entropy analysis
    pub enable_entropy: bool,
    
    /// Enable structure analysis
    pub enable_structure: bool,
    
    /// Enable content analysis
    pub enable_content: bool,
    
    /// Custom patterns to match
    pub custom_patterns: Vec<String>,
}

impl Default for PatternConfig {
    fn default() -> Self {
        Self {
            enable_entropy: true,
            enable_structure: true,
            enable_content: true,
            custom_patterns: Vec::new(),
        }
    }
}

/// Pattern analysis results
#[derive(Debug)]
pub struct PatternResults {
    /// Matched patterns
    pub matches: Vec<PatternMatch>,
    
    /// Analysis statistics
    pub statistics: PatternStatistics,
}

/// Pattern match information
#[derive(Debug)]
pub struct PatternMatch {
    /// Pattern identifier
    pub pattern_id: String,
    
    /// Match location
    pub location: MatchLocation,
    
    /// Match confidence (0.0 - 1.0)
    pub confidence: f64,
    
    /// Additional context
    pub context: String,
}

/// Pattern match location
#[derive(Debug)]
pub enum MatchLocation {
    /// Match in stream
    Stream {
        object_id: crate::types::ObjectId,
        offset: usize,
    },
    /// Match in string
    String {
        object_id: crate::types::ObjectId,
    },
    /// Match in metadata
    Metadata {
        field: String,
    },
}

/// Pattern analysis statistics
#[derive(Debug, Default)]
pub struct PatternStatistics {
    /// Number of patterns analyzed
    pub patterns_analyzed: usize,
    
    /// Number of matches found
    pub matches_found: usize,
    
    /// Analysis duration in milliseconds
    pub duration_ms: u64,
}


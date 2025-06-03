//! Pattern database implementation for anti-forensics
//! Created: 2025-06-03 13:57:56 UTC
//! Author: kartik4091

use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::Path,
};

use serde::{Deserialize, Serialize};
use crate::error::{Error, Result};
use super::matcher::PatternType;

/// Pattern database for forensic artifact detection
#[derive(Debug, Clone)]
pub struct PatternDatabase {
    /// Patterns indexed by ID
    patterns: Vec<Pattern>,
    
    /// Pattern categories
    categories: HashMap<String, Category>,
    
    /// Pattern metadata
    metadata: DatabaseMetadata,
}

/// Pattern definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    /// Unique pattern identifier
    pub id: String,
    
    /// Pattern name
    pub name: String,
    
    /// Pattern description
    pub description: String,
    
    /// Pattern category
    pub category: String,
    
    /// Pattern type (regex or string)
    #[serde(rename = "type")]
    pub pattern_type: PatternType,
    
    /// Pattern string
    pub pattern: String,
    
    /// Pattern severity level
    pub severity: SeverityLevel,
    
    /// MIME types this pattern applies to
    #[serde(default)]
    pub mime_types: Vec<String>,
    
    /// Pattern tags
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Pattern category
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    /// Category name
    pub name: String,
    
    /// Category description
    pub description: String,
    
    /// Parent category (if any)
    pub parent: Option<String>,
}

/// Pattern database metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseMetadata {
    /// Database version
    pub version: String,
    
    /// Database description
    pub description: String,
    
    /// Last updated timestamp
    pub last_updated: String,
    
    /// Database maintainer
    pub maintainer: String,
}

/// Pattern severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SeverityLevel {
    /// Informational finding
    Info,
    /// Low severity
    Low,
    /// Medium severity
    Medium,
    /// High severity
    High,
    /// Critical severity
    Critical,
}

impl PatternDatabase {
    /// Load pattern database from a file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = File::open(path)
            .map_err(|e| Error::pattern(format!("Failed to open pattern database: {}", e)))?;
            
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .map_err(|e| Error::pattern(format!("Failed to read pattern database: {}", e)))?;
            
        Self::from_str(&contents)
    }
    
    /// Load pattern database from a string
    pub fn from_str(s: &str) -> Result<Self> {
        let db: DatabaseFile = serde_json::from_str(s)
            .map_err(|e| Error::pattern(format!("Failed to parse pattern database: {}", e)))?;
            
        // Convert to internal representation
        let patterns = db.patterns;
        let categories = db.categories.into_iter()
            .map(|c| (c.name.clone(), c))
            .collect();
            
        Ok(Self {
            patterns,
            categories,
            metadata: db.metadata,
        })
    }
    
    /// Get all patterns
    pub fn patterns(&self) -> &[Pattern] {
        &self.patterns
    }
    
    /// Get patterns by category
    pub fn patterns_by_category(&self, category: &str) -> Vec<&Pattern> {
        self.patterns.iter()
            .filter(|p| p.category == category)
            .collect()
    }
    
    /// Get patterns by severity
    pub fn patterns_by_severity(&self, severity: SeverityLevel) -> Vec<&Pattern> {
        self.patterns.iter()
            .filter(|p| p.severity == severity)
            .collect()
    }
    
    /// Get patterns by MIME type
    pub fn patterns_by_mime_type(&self, mime_type: &str) -> Vec<&Pattern> {
        self.patterns.iter()
            .filter(|p| p.mime_types.iter().any(|m| m == mime_type))
            .collect()
    }
    
    /// Get patterns by tag
    pub fn patterns_by_tag(&self, tag: &str) -> Vec<&Pattern> {
        self.patterns.iter()
            .filter(|p| p.tags.iter().any(|t| t == tag))
            .collect()
    }
    
    /// Get database metadata
    pub fn metadata(&self) -> &DatabaseMetadata {
        &self.metadata
    }
    
    /// Get category by name
    pub fn category(&self, name: &str) -> Option<&Category> {
        self.categories.get(name)
    }
    
    /// Get all categories
    pub fn categories(&self) -> impl Iterator<Item = &Category> {
        self.categories.values()
    }
}

/// Pattern database file format
#[derive(Debug, Serialize, Deserialize)]
struct DatabaseFile {
    /// Database metadata
    metadata: DatabaseMetadata,
    
    /// Pattern definitions
    patterns: Vec<Pattern>,
    
    /// Category definitions
    categories: Vec<Category>,
}

impl Default for PatternDatabase {
    fn default() -> Self {
        Self {
            patterns: Vec::new(),
            categories: HashMap::new(),
            metadata: DatabaseMetadata {
                version: String::from("1.0.0"),
                description: String::from("Default empty pattern database"),
                last_updated: String::from("2025-06-03 13:57:56"),
                maintainer: String::from("kartik4091"),
            },
        }
    }
              }

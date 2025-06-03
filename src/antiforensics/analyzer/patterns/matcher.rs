//! Pattern matching implementation for anti-forensics
//! Created: 2025-06-03 13:56:04 UTC
//! Author: kartik4091

use std::collections::HashMap;
use aho_corasick::AhoCorasick;
use regex::Regex;

use super::{
    PatternMatch,
    MatchLocation,
    PatternResults,
    PatternStatistics,
    database::PatternDatabase,
};

use crate::{
    error::{Error, Result},
    types::{Document, Object, ObjectId},
};

/// Pattern matcher for detecting forensic artifacts
pub struct PatternMatcher {
    /// Pattern database
    database: PatternDatabase,
    
    /// Compiled regular expressions
    regex_patterns: HashMap<String, Regex>,
    
    /// Aho-Corasick automaton for string matching
    string_matcher: AhoCorasick,
    
    /// Match statistics
    statistics: PatternStatistics,
}

impl PatternMatcher {
    /// Create a new pattern matcher
    pub fn new(database: PatternDatabase) -> Result<Self> {
        let mut regex_patterns = HashMap::new();
        let mut string_patterns = Vec::new();
        
        // Compile regex patterns and collect string patterns
        for pattern in database.patterns() {
            match pattern.pattern_type {
                PatternType::Regex => {
                    let regex = Regex::new(&pattern.pattern)
                        .map_err(|e| Error::pattern(format!("Invalid regex pattern: {}", e)))?;
                    regex_patterns.insert(pattern.id.clone(), regex);
                },
                PatternType::String => {
                    string_patterns.push(pattern.pattern.as_bytes());
                }
            }
        }
        
        // Create Aho-Corasick automaton for string matching
        let string_matcher = AhoCorasick::new(string_patterns)
            .map_err(|e| Error::pattern(format!("Failed to create string matcher: {}", e)))?;
            
        Ok(Self {
            database,
            regex_patterns,
            string_matcher,
            statistics: PatternStatistics::default(),
        })
    }
    
    /// Match patterns in a document
    pub fn match_patterns(&mut self, document: &Document) -> Result<PatternResults> {
        let start_time = std::time::Instant::now();
        let mut matches = Vec::new();
        
        // Match patterns in streams
        for (object_id, object) in &document.structure.objects {
            if let Object::Stream { dict: _, data } = object {
                self.match_stream(object_id, data, &mut matches)?;
            }
        }
        
        // Match patterns in strings
        for (object_id, object) in &document.structure.objects {
            if let Object::String(data) = object {
                self.match_string(object_id, data, &mut matches)?;
            }
        }
        
        // Match patterns in metadata
        if let Some(metadata) = &document.metadata {
            self.match_metadata(metadata, &mut matches)?;
        }
        
        // Update statistics
        self.statistics.patterns_analyzed += self.database.patterns().len();
        self.statistics.matches_found += matches.len();
        self.statistics.duration_ms = start_time.elapsed().as_millis() as u64;
        
        Ok(PatternResults {
            matches,
            statistics: self.statistics.clone(),
        })
    }
    
    /// Match patterns in a stream
    fn match_stream(&self, object_id: &ObjectId, data: &[u8], matches: &mut Vec<PatternMatch>) -> Result<()> {
        // Match regex patterns
        for (id, regex) in &self.regex_patterns {
            for mat in regex.find_iter(data) {
                matches.push(PatternMatch {
                    pattern_id: id.clone(),
                    location: MatchLocation::Stream {
                        object_id: *object_id,
                        offset: mat.start(),
                    },
                    confidence: 1.0,
                    context: String::from_utf8_lossy(&data[mat.start()..mat.end()]).into_owned(),
                });
            }
        }
        
        // Match string patterns
        for mat in self.string_matcher.find_iter(data) {
            let pattern = &self.database.patterns()[mat.pattern()];
            matches.push(PatternMatch {
                pattern_id: pattern.id.clone(),
                location: MatchLocation::Stream {
                    object_id: *object_id,
                    offset: mat.start(),
                },
                confidence: 1.0,
                context: String::from_utf8_lossy(&data[mat.start()..mat.end()]).into_owned(),
            });
        }
        
        Ok(())
    }
    
    /// Match patterns in a string object
    fn match_string(&self, object_id: &ObjectId, data: &[u8], matches: &mut Vec<PatternMatch>) -> Result<()> {
        // Match regex patterns
        for (id, regex) in &self.regex_patterns {
            if regex.is_match(data) {
                matches.push(PatternMatch {
                    pattern_id: id.clone(),
                    location: MatchLocation::String {
                        object_id: *object_id,
                    },
                    confidence: 1.0,
                    context: String::from_utf8_lossy(data).into_owned(),
                });
            }
        }
        
        // Match string patterns
        if self.string_matcher.find_iter(data).next().is_some() {
            for mat in self.string_matcher.find_iter(data) {
                let pattern = &self.database.patterns()[mat.pattern()];
                matches.push(PatternMatch {
                    pattern_id: pattern.id.clone(),
                    location: MatchLocation::String {
                        object_id: *object_id,
                    },
                    confidence: 1.0,
                    context: String::from_utf8_lossy(data).into_owned(),
                });
            }
        }
        
        Ok(())
    }
    
    /// Match patterns in metadata
    fn match_metadata(&self, metadata: &super::super::types::DocumentMetadata, matches: &mut Vec<PatternMatch>) -> Result<()> {
        // Helper function to check field
        let check_field = |field: &str, value: &str, matches: &mut Vec<PatternMatch>| {
            // Match regex patterns
            for (id, regex) in &self.regex_patterns {
                if regex.is_match(value.as_bytes()) {
                    matches.push(PatternMatch {
                        pattern_id: id.clone(),
                        location: MatchLocation::Metadata {
                            field: field.to_string(),
                        },
                        confidence: 1.0,
                        context: value.to_string(),
                    });
                }
            }
            
            // Match string patterns
            if self.string_matcher.find_iter(value.as_bytes()).next().is_some() {
                for mat in self.string_matcher.find_iter(value.as_bytes()) {
                    let pattern = &self.database.patterns()[mat.pattern()];
                    matches.push(PatternMatch {
                        pattern_id: pattern.id.clone(),
                        location: MatchLocation::Metadata {
                            field: field.to_string(),
                        },
                        confidence: 1.0,
                        context: value.to_string(),
                    });
                }
            }
        };
        
        // Check each metadata field
        if let Some(producer) = &metadata.producer {
            check_field("Producer", producer, matches);
        }
        if let Some(creator) = &metadata.creator {
            check_field("Creator", creator, matches);
        }
        if let Some(title) = &metadata.title {
            check_field("Title", title, matches);
        }
        if let Some(author) = &metadata.author {
            check_field("Author", author, matches);
        }
        if let Some(subject) = &metadata.subject {
            check_field("Subject", subject, matches);
        }
        
        Ok(())
    }
}

/// Pattern type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternType {
    /// Regular expression pattern
    Regex,
    /// String literal pattern
    String,
      }

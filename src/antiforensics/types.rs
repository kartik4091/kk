//! Core types for PDF anti-forensics operations
//! Author: kartik4091
//! Created: 2025-06-03 10:19:43 UTC
//! This module defines common types used throughout the anti-forensics system.

use std::{
    collections::HashMap,
    fmt::{self, Display},
    path::PathBuf,
    time::Duration,
};
use serde::{Deserialize, Serialize};

/// Verification level for processing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationLevel {
    /// Basic verification
    Basic,
    /// Standard verification
    Standard,
    /// Thorough verification
    Thorough,
    /// Paranoid verification
    Paranoid,
}

/// Risk level for detected issues
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    /// Low risk
    Low,
    /// Medium risk
    Medium,
    /// High risk
    High,
    /// Critical risk
    Critical,
}

/// Processing stage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessingStage {
    /// Initial verification
    Verification,
    /// Structure analysis
    StructureAnalysis,
    /// Deep cleaning
    DeepCleaning,
    /// Content processing
    ContentProcessing,
    /// Metadata handling
    MetadataHandling,
    /// Security implementation
    SecurityImplementation,
    /// Forensic verification
    ForensicVerification,
    /// Output generation
    OutputGeneration,
}

/// Processing status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessingStatus {
    /// Not started
    NotStarted,
    /// In progress
    InProgress,
    /// Completed
    Completed,
    /// Failed
    Failed,
    /// Cancelled
    Cancelled,
}

/// User metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserMetadata {
    /// Document title
    pub title: Option<String>,
    /// Document author
    pub author: Option<String>,
    /// Document subject
    pub subject: Option<String>,
    /// Document keywords
    pub keywords: Option<String>,
}

/// Security options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityOptions {
    /// Enable encryption
    pub encryption_enabled: bool,
    /// User password
    pub user_password: Option<String>,
    /// Owner password
    pub owner_password: Option<String>,
    /// Document permissions
    pub permissions: Permissions,
}

/// Document permissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permissions {
    /// Allow printing
    pub print_allowed: bool,
    /// Allow modifications
    pub modify_allowed: bool,
    /// Allow copying
    pub copy_allowed: bool,
    /// Allow annotations
    pub annotate_allowed: bool,
}

/// Processing metrics
#[derive(Debug, Clone, Default)]
pub struct ProcessingMetrics {
    /// Processing duration
    pub duration: Duration,
    /// Items processed
    pub items_processed: u64,
    /// Errors encountered
    pub errors: u64,
    /// Memory usage
    pub memory_usage: u64,
    /// Stage metrics
    pub stage_metrics: HashMap<ProcessingStage, StageMetrics>,
}

/// Stage-specific metrics
#[derive(Debug, Clone, Default)]
pub struct StageMetrics {
    /// Stage duration
    pub duration: Duration,
    /// Items processed
    pub items_processed: u64,
    /// Memory usage
    pub memory_usage: u64,
}

/// Display implementations
impl Display for VerificationLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Basic => write!(f, "Basic"),
            Self::Standard => write!(f, "Standard"),
            Self::Thorough => write!(f, "Thorough"),
            Self::Paranoid => write!(f, "Paranoid"),
        }
    }
}

impl Display for RiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => write!(f, "Low"),
            Self::Medium => write!(f, "Medium"),
            Self::High => write!(f, "High"),
            Self::Critical => write!(f, "Critical"),
        }
    }
}

impl Display for ProcessingStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Verification => write!(f, "Verification"),
            Self::StructureAnalysis => write!(f, "Structure Analysis"),
            Self::DeepCleaning => write!(f, "Deep Cleaning"),
            Self::ContentProcessing => write!(f, "Content Processing"),
            Self::MetadataHandling => write!(f, "Metadata Handling"),
            Self::SecurityImplementation => write!(f, "Security Implementation"),
            Self::ForensicVerification => write!(f, "Forensic Verification"),
            Self::OutputGeneration => write!(f, "Output Generation"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verification_level_display() {
        assert_eq!(VerificationLevel::Basic.to_string(), "Basic");
        assert_eq!(VerificationLevel::Standard.to_string(), "Standard");
        assert_eq!(VerificationLevel::Thorough.to_string(), "Thorough");
        assert_eq!(VerificationLevel::Paranoid.to_string(), "Paranoid");
    }

    #[test]
    fn test_risk_level_display() {
        assert_eq!(RiskLevel::Low.to_string(), "Low");
        assert_eq!(RiskLevel::Medium.to_string(), "Medium");
        assert_eq!(RiskLevel::High.to_string(), "High");
        assert_eq!(RiskLevel::Critical.to_string(), "Critical");
    }

    #[test]
    fn test_processing_stage_display() {
        assert_eq!(ProcessingStage::Verification.to_string(), "Verification");
        assert_eq!(ProcessingStage::DeepCleaning.to_string(), "Deep Cleaning");
        assert_eq!(ProcessingStage::OutputGeneration.to_string(), "Output Generation");
    }

    #[test]
    fn test_user_metadata_default() {
        let metadata = UserMetadata::default();
        assert!(metadata.title.is_none());
        assert!(metadata.author.is_none());
        assert!(metadata.subject.is_none());
        assert!(metadata.keywords.is_none());
    }
          }

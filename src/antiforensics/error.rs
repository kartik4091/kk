//! Error types for PDF anti-forensics operations
//! Author: kartik4091
//! Created: 2025-06-03 10:19:43 UTC
//! This module defines all error types used throughout the anti-forensics system.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    io,
    result,
    sync::PoisonError,
};
use thiserror::Error;

/// Type alias for Result with ForensicError
pub type Result<T> = result::Result<T, ForensicError>;

/// Main error type for anti-forensics operations
#[derive(Error, Debug)]
pub enum ForensicError {
    /// Input/Output errors
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// PDF parsing errors
    #[error("PDF parsing error: {0}")]
    PdfParse(String),

    /// Validation errors
    #[error("Validation error: {0}")]
    Validation(String),

    /// Security errors
    #[error("Security error: {0}")]
    Security(String),

    /// Metadata errors
    #[error("Metadata error: {0}")]
    Metadata(String),

    /// Resource errors
    #[error("Resource error: {0}")]
    Resource(String),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// State errors
    #[error("State error: {0}")]
    State(String),

    /// Concurrency errors
    #[error("Concurrency error: {0}")]
    Concurrency(String),

    /// Processing errors
    #[error("Processing error: {0}")]
    Processing(String),

    /// Verification errors
    #[error("Verification error: {0}")]
    Verification(String),

    /// Cleanup errors
    #[error("Cleanup error: {0}")]
    Cleanup(String),
}

/// Error type for PDF structure operations
#[derive(Error, Debug)]
pub enum StructureError {
    /// Invalid PDF version
    #[error("Invalid PDF version: {0}")]
    InvalidVersion(String),

    /// Cross-reference table errors
    #[error("Cross-reference error: {0}")]
    XrefError(String),

    /// Trailer dictionary errors
    #[error("Trailer error: {0}")]
    TrailerError(String),

    /// Object structure errors
    #[error("Object error: {0}")]
    ObjectError(String),

    /// Stream errors
    #[error("Stream error: {0}")]
    StreamError(String),
}

/// Error type for cleanup operations
#[derive(Error, Debug)]
pub enum CleanupError {
    /// Stream cleaning errors
    #[error("Stream cleaning error: {0}")]
    StreamError(String),

    /// Binary cleaning errors
    #[error("Binary cleaning error: {0}")]
    BinaryError(String),

    /// Content cleaning errors
    #[error("Content cleaning error: {0}")]
    ContentError(String),

    /// Structure cleaning errors
    #[error("Structure cleaning error: {0}")]
    StructureError(String),
}

/// Error type for security operations
#[derive(Error, Debug)]
pub enum SecurityError {
    /// Encryption errors
    #[error("Encryption error: {0}")]
    EncryptionError(String),

    /// Permission errors
    #[error("Permission error: {0}")]
    PermissionError(String),

    /// Signature errors
    #[error("Signature error: {0}")]
    SignatureError(String),

    /// Authentication errors
    #[error("Authentication error: {0}")]
    AuthError(String),
}

/// Error type for metadata operations
#[derive(Error, Debug)]
pub enum MetadataError {
    /// Info dictionary errors
    #[error("Info dictionary error: {0}")]
    InfoError(String),

    /// XMP metadata errors
    #[error("XMP error: {0}")]
    XmpError(String),

    /// Document ID errors
    #[error("Document ID error: {0}")]
    DocIdError(String),

    /// Timestamp errors
    #[error("Timestamp error: {0}")]
    TimestampError(String),
}

/// Error conversion implementations
impl From<StructureError> for ForensicError {
    fn from(err: StructureError) -> Self {
        ForensicError::PdfParse(err.to_string())
    }
}

impl From<CleanupError> for ForensicError {
    fn from(err: CleanupError) -> Self {
        ForensicError::Processing(err.to_string())
    }
}

impl From<SecurityError> for ForensicError {
    fn from(err: SecurityError) -> Self {
        ForensicError::Security(err.to_string())
    }
}

impl From<MetadataError> for ForensicError {
    fn from(err: MetadataError) -> Self {
        ForensicError::Metadata(err.to_string())
    }
}

/// Thread safety error conversions
impl<T> From<PoisonError<T>> for ForensicError {
    fn from(err: PoisonError<T>) -> Self {
        ForensicError::Concurrency(format!("Lock poisoned: {}", err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_conversion() {
        let structure_err = StructureError::InvalidVersion("1.8".to_string());
        let forensic_err: ForensicError = structure_err.into();
        assert!(matches!(forensic_err, ForensicError::PdfParse(_)));

        let cleanup_err = CleanupError::StreamError("Failed to clean stream".to_string());
        let forensic_err: ForensicError = cleanup_err.into();
        assert!(matches!(forensic_err, ForensicError::Processing(_)));
    }

    #[test]
    fn test_error_display() {
        let err = ForensicError::Security("Invalid password".to_string());
        assert_eq!(err.to_string(), "Security error: Invalid password");

        let err = MetadataError::XmpError("Invalid XMP".to_string());
        assert_eq!(err.to_string(), "XMP error: Invalid XMP");
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "File not found");
        let forensic_err: ForensicError = io_err.into();
        assert!(matches!(forensic_err, ForensicError::Io(_)));
    }
  }

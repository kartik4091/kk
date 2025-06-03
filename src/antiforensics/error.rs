//! Error types and handling for antiforensics library
//! Created: 2025-06-03 11:31:05 UTC
//! Author: kartik4091

use std::{
    error::Error as StdError,
    fmt::{Display, Formatter, Result as FmtResult},
    io,
    result::Result as StdResult,
    sync::PoisonError,
};
use thiserror::Error;
use tokio::sync::TryLockError;
use tracing::error;

/// Custom result type for antiforensics operations
pub type Result<T> = StdResult<T, Error>;

/// Core error type for antiforensics operations
#[derive(Error, Debug)]
pub enum Error {
    #[error("Initialization error: {0}")]
    InitializationError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("PDF structure error: {0}")]
    StructureError(#[from] StructureError),

    #[error("Analysis error: {0}")]
    AnalysisError(#[from] AnalysisError),

    #[error("Cleaner error: {0}")]
    CleanerError(#[from] CleanerError),

    #[error("Encryption error: {0}")]
    EncryptionError(#[from] EncryptionError),

    #[error("Hash error: {0}")]
    HashError(#[from] HashError),

    #[error("Scanner error: {0}")]
    ScannerError(#[from] ScannerError),

    #[error("Stego error: {0}")]
    StegoError(#[from] StegoError),

    #[error("Verification error: {0}")]
    VerificationError(#[from] VerificationError),

    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),

    #[error("Concurrency error: {0}")]
    ConcurrencyError(String),

    #[error("Resource error: {0}")]
    ResourceError(String),

    #[error("Timeout error: {0}")]
    TimeoutError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}

/// PDF structure specific errors
#[derive(Error, Debug)]
pub enum StructureError {
    #[error("Invalid PDF header: {0}")]
    InvalidHeader(String),

    #[error("Invalid xref table: {0}")]
    InvalidXref(String),

    #[error("Invalid trailer: {0}")]
    InvalidTrailer(String),

    #[error("Invalid object stream: {0}")]
    InvalidObjectStream(String),

    #[error("Missing required object: {0}")]
    MissingObject(String),

    #[error("Corrupted structure: {0}")]
    Corrupted(String),
}

/// Analysis specific errors
#[derive(Error, Debug)]
pub enum AnalysisError {
    #[error("Pattern analysis failed: {0}")]
    PatternError(String),

    #[error("Content analysis failed: {0}")]
    ContentError(String),

    #[error("Metadata analysis failed: {0}")]
    MetadataError(String),

    #[error("Version analysis failed: {0}")]
    VersionError(String),
}

/// Cleaner specific errors
#[derive(Error, Debug)]
pub enum CleanerError {
    #[error("Metadata cleaning failed: {0}")]
    MetadataError(String),

    #[error("Content cleaning failed: {0}")]
    ContentError(String),

    #[error("Structure cleaning failed: {0}")]
    StructureError(String),

    #[error("Stream cleaning failed: {0}")]
    StreamError(String),
}

/// Encryption specific errors
#[derive(Error, Debug)]
pub enum EncryptionError {
    #[error("Key generation failed: {0}")]
    KeyGenError(String),

    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("Invalid algorithm: {0}")]
    InvalidAlgorithm(String),
}

/// Hash specific errors
#[derive(Error, Debug)]
pub enum HashError {
    #[error("Hash computation failed: {0}")]
    ComputationError(String),

    #[error("Invalid hash format: {0}")]
    InvalidFormat(String),

    #[error("Hash verification failed: {0}")]
    VerificationError(String),
}

/// Scanner specific errors
#[derive(Error, Debug)]
pub enum ScannerError {
    #[error("File scan failed: {0}")]
    FileScanError(String),

    #[error("Memory scan failed: {0}")]
    MemoryScanError(String),

    #[error("Network scan failed: {0}")]
    NetworkScanError(String),

    #[error("Process scan failed: {0}")]
    ProcessScanError(String),
}

/// Steganography specific errors
#[derive(Error, Debug)]
pub enum StegoError {
    #[error("Data embedding failed: {0}")]
    EmbedError(String),

    #[error("Data extraction failed: {0}")]
    ExtractError(String),

    #[error("Capacity error: {0}")]
    CapacityError(String),
}

/// Verification specific errors
#[derive(Error, Debug)]
pub enum VerificationError {
    #[error("Signature verification failed: {0}")]
    SignatureError(String),

    #[error("Certificate verification failed: {0}")]
    CertificateError(String),

    #[error("Integrity check failed: {0}")]
    IntegrityError(String),
}

// Implement conversions for common error types
impl<T> From<PoisonError<T>> for Error {
    fn from(err: PoisonError<T>) -> Self {
        Error::ConcurrencyError(format!("Lock poison error: {}", err))
    }
}

impl<T> From<TryLockError<T>> for Error {
    fn from(err: TryLockError<T>) -> Self {
        Error::ConcurrencyError(format!("Lock acquisition error: {}", err))
    }
}

// Error context trait for adding context to errors
pub trait ErrorContext<T, E> {
    fn context(self, context: impl Display) -> StdResult<T, Error>;
    fn with_context<F>(self, f: F) -> StdResult<T, Error>
    where
        F: FnOnce() -> String;
}

impl<T, E: StdError + 'static> ErrorContext<T, E> for StdResult<T, E> {
    fn context(self, context: impl Display) -> StdResult<T, Error> {
        self.map_err(|e| {
            error!("{}: {}", context, e);
            Error::InternalError(format!("{}: {}", context, e))
        })
    }

    fn with_context<F>(self, f: F) -> StdResult<T, Error>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| {
            let context = f();
            error!("{}: {}", context, e);
            Error::InternalError(format!("{}: {}", context, e))
        })
    }
}

// Testing module
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::InitializationError("test error".into());
        assert_eq!(err.to_string(), "Initialization error: test error");
    }

    #[test]
    fn test_error_context() {
        let result: StdResult<(), std::io::Error> = 
            Err(std::io::Error::new(std::io::ErrorKind::Other, "test error"));
        
        let with_context = result.context("Operation failed");
        assert!(matches!(with_context, Err(Error::InternalError(_))));
    }

    #[test]
    fn test_error_conversion() {
        let io_error = io::Error::new(io::ErrorKind::Other, "test error");
        let err: Error = io_error.into();
        assert!(matches!(err, Error::IoError(_)));
    }

    #[test]
    fn test_structured_error_handling() {
        let structure_err = StructureError::InvalidHeader("Invalid PDF version".into());
        let err: Error = structure_err.into();
        assert!(matches!(err, Error::StructureError(_)));
    }
}

// Public error utilities
pub mod utils {
    use super::*;

    /// Log and convert an error with context
    pub fn log_error<E: StdError>(error: E, context: &str) -> Error {
        error!("{}: {}", context, error);
        Error::InternalError(format!("{}: {}", context, error))
    }

    /// Create a validation error with the given message
    pub fn validation_error(msg: impl Into<String>) -> Error {
        Error::ValidationError(msg.into())
    }

    /// Create a resource error with the given message
    pub fn resource_error(msg: impl Into<String>) -> Error {
        Error::ResourceError(msg.into())
    }
        }

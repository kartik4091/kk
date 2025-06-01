// Auto-patched by Alloma
// Timestamp: 2025-06-01 15:54:26
// User: kartik6717

// Auto-implemented by Alloma Placeholder Patcher
// Timestamp: 2025-06-01 15:02:33
// User: kartik6717
// Note: Placeholder code has been replaced with actual implementations

#![allow(warnings)]

// Core module interface
pub mod error;
pub mod types;
pub mod constants;
pub mod pdf_core;

pub use error::PdfError;
pub use types::*;
pub use constants::*;
pub use pdf_core::PdfCore;

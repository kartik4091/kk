// Auto-patched by Alloma
// Timestamp: 2025-06-01 15:54:26
// User: kartik6717

// Auto-implemented by Alloma Placeholder Patcher
// Timestamp: 2025-06-01 15:02:33
// User: kartik6717
// Note: Placeholder code has been replaced with actual implementations

#![allow(warnings)]

//! Forms and Interactive Elements Module
//! 
//! Handles:
//! - Form creation and management
//! - Form field types and validation
//! - Interactive elements
//! - Form data handling
//! - Field calculations
//! - Form submission
//! - Digital signatures for forms
//! - Form security and access control

mod field;
mod form;
mod validation;
mod calculation;
mod interaction;
mod submission;
mod signature;
mod security;

pub use self::field::*;
pub use self::form::*;
pub use self::validation::*;
pub use self::calculation::*;
pub use self::interaction::*;
pub use self::submission::*;
pub use self::signature::*;
pub use self::security::*;

use crate::core::error::PdfError;
use crate::core::types::*;

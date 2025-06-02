// Auto-patched by Alloma
// Timestamp: 2025-06-01 23:53:15
// User: kartik4091

#![allow(warnings)]

use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use crate::core::error::PdfError;

pub mod pdfa;
pub mod pdfx;
pub mod digital;
pub mod print;
pub mod archive;
pub mod version;
pub mod conversion;
pub mod batch;
pub mod profiles;
pub mod quality;

// Re-exports
pub use pdfa::PdfAManager;
pub use pdfx::PdfXManager;
pub use digital::DigitalPublisher;
pub use print::PrintProductionManager;
pub use archive::ArchiveManager;
pub use version::VersionManager;
pub use conversion::FormatConverter;
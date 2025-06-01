use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

// Module declarations
pub mod core;
pub mod writer;
pub mod version;
pub mod metadata;
pub mod security;
pub mod forensics;
pub mod verification;
pub mod processing;

pub use crate::core::Document;
pub use crate::metadata::{clean_docinfo_metadata, remove_xmp_metadata};
pub use crate::writer::save_document;

// Error handling
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("PDF processing error: {0}")]
    Processing(String),

    #[error("Version error: {0}")]
    Version(String),

    #[error("Metadata error: {0}")]
    Metadata(String),

    #[error("Security error: {0}")]
    Security(String),

    #[error("Forensic error: {0}")]
    Forensic(String),

    #[error("Verification error: {0}")]
    Verification(String),
}

// Configuration structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessConfig {
    pub clean_metadata: bool,
    pub normalize_version: bool,
    pub apply_security: bool,
    pub forensic_clean: bool,
    pub security: SecurityConfig,
    pub user_metadata: Option<BTreeMap<String, String>>, // ✅ Custom metadata injection
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub encrypt: bool,
    pub user_password: Option<String>,
    pub owner_password: Option<String>,
    pub permissions: Vec<String>,
}

// Main PDF Engine structure
#[derive(Debug)]
pub struct PdfEngine {
    context: EngineContext,
    state: Arc<RwLock<EngineState>>,
    core: Arc<core::CoreSystem>,
    version: Arc<version::VersionSystem>,
    metadata: Arc<metadata::MetadataSystem>,
    security: Arc<security::SecuritySystem>,
    forensics: Arc<forensics::ForensicSystem>,
    verification: Arc<verification::VerificationSystem>,
}

#[derive(Debug, Clone)]
struct EngineContext {
    timestamp: DateTime<Utc>,
    user: String,
    version: String,
}

#[derive(Debug)]
struct EngineState {
    processing_count: u64,
    last_processed: Option<DateTime<Utc>>,
    active_tasks: u32,
}

impl PdfEngine {
    pub async fn new() -> Result<Self, Error> {
        let context = EngineContext {
            timestamp: DateTime::from_utc(
                chrono::NaiveDateTime::parse_from_str(
                    "2025-05-31 19:58:03",
                    "%Y-%m-%d %H:%M:%S"
                ).unwrap(),
                Utc,
            ),
            user: "kartik6717".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        };

        let state = Arc::new(RwLock::new(EngineState {
            processing_count: 0,
            last_processed: None,
            active_tasks: 0,
        }));

        Ok(Self {
            context,
            state,
            core: Arc::new(core::CoreSystem::new().await?),
            version: Arc::new(version::VersionSystem::new().await?),
            metadata: Arc::new(metadata::MetadataSystem::new().await?),
            security: Arc::new(security::SecuritySystem::new().await?),
            forensics: Arc::new(forensics::ForensicSystem::new().await?),
            verification: Arc::new(verification::VerificationSystem::new().await?),
        })
    }

    pub async fn process_document(&self, input: &[u8], config: ProcessConfig) -> Result<Vec<u8>, Error> {
        let mut state = self.state.write().await;
        state.active_tasks += 1;
        state.processing_count += 1;

        println!("Starting document processing");
        println!("Timestamp: {}", self.context.timestamp);
        println!("User: {}", self.context.user);

        let mut document = self.core.parse_document(input).await?;

        if config.normalize_version {
            self.version.normalize_to_1_4(&mut document).await?;
        }

        if config.clean_metadata {
            self.metadata.clean_metadata(&mut document).await?;
        }

        // ✅ Inject user-defined metadata if present
        if let Some(ref meta) = config.user_metadata {
            self.metadata.inject_metadata(&mut document, meta.clone()).await?;
        }

        if config.apply_security {
            self.security.apply_security(&mut document, &config.security).await?;
        }

        if config.forensic_clean {
            self.forensics.clean_document(&mut document).await?;
        }

        self.verification.verify_document(&document).await?;

        state.last_processed = Some(Utc::now());
        state.active_tasks -= 1;

        self.core.write_document(&document).await
    }

    pub async fn get_stats(&self) -> Result<EngineStats, Error> {
        let state = self.state.read().await;
        Ok(EngineStats {
            total_processed: state.processing_count,
            active_tasks: state.active_tasks,
            last_processed: state.last_processed,
            uptime: Utc::now() - self.context.timestamp,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct EngineStats {
    total_processed: u64,
    active_tasks: u32,
    last_processed: Option<DateTime<Utc>>,
    uptime: chrono::Duration,
}

// ✅ Re-exports for convenience
pub use crate::core::Document;
pub use self::Error;
//! Entropy analysis module
//! Created: 2025-06-03 12:36:51 UTC
//! Author: kartik4091

mod shannon;
mod algorithms;

pub use self::{
    shannon::{ShannonEntropy, EntropyResult},
    algorithms::{EntropyAlgorithms, AlgorithmResults},
};

use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, instrument};

use crate::{
    error::{Error, Result},
    types::Document,
};

/// Combined entropy analysis results
#[derive(Debug, Clone)]
pub struct EntropyAnalysis {
    /// Shannon entropy results
    pub shannon: EntropyResult,
    /// Additional algorithm results
    pub algorithms: AlgorithmResults,
    /// Analysis timestamp
    pub timestamp: std::time::Instant,
}

/// Main entropy analyzer combining multiple approaches
pub struct EntropyAnalyzer {
    /// Shannon entropy calculator
    shannon: ShannonEntropy,
    /// Additional entropy algorithms
    algorithms: EntropyAlgorithms,
    /// Processing limiter
    limiter: Arc<Semaphore>,
}

impl EntropyAnalyzer {
    /// Creates new entropy analyzer
    pub fn new(window_size: usize, window_overlap: usize, dict_size: usize) -> Self {
        Self {
            shannon: ShannonEntropy::new(window_size, window_overlap),
            algorithms: EntropyAlgorithms::new(dict_size),
            limiter: Arc::new(Semaphore::new(num_cpus::get())),
        }
    }

    /// Performs complete entropy analysis
    #[instrument(skip(self, document))]
    pub async fn analyze(&self, document: &Document) -> Result<EntropyAnalysis> {
        let _permit = self.limiter.acquire().await?;
        let start = std::time::Instant::now();

        // Run both analyses in parallel
        let (shannon_result, algorithm_result) = tokio::join!(
            self.shannon.analyze(document),
            self.algorithms.analyze(document)
        );

        Ok(EntropyAnalysis {
            shannon: shannon_result?,
            algorithms: algorithm_result?,
            timestamp: start,
        })
    }

    /// Gets Shannon entropy analyzer
    pub fn shannon(&self) -> &ShannonEntropy {
        &self.shannon
    }

    /// Gets additional entropy algorithms
    pub fn algorithms(&self) -> &EntropyAlgorithms {
        &self.algorithms
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_combined_analysis() {
        let data = b"Test data for entropy analysis";
        let file = NamedTempFile::new().unwrap();
        tokio::fs::write(&file, data).await.unwrap();
        
        let doc = Document::new(file.path().to_path_buf(), data.len() as u64);
        let analyzer = EntropyAnalyzer::new(8, 4, 256);
        
        let result = analyzer.analyze(&doc).await.unwrap();
        assert!(result.shannon.entropy > 0.0);
        assert!(result.algorithms.compression_ratio > 0.0);
    }

    #[test]
    fn test_component_access() {
        let analyzer = EntropyAnalyzer::new(8, 4, 256);
        assert!(analyzer.shannon().get_stats().blocks_processed >= 0);
    }
  }

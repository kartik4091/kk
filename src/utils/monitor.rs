// Auto-patched by Alloma
// Timestamp: 2025-06-02 00:10:07
// User: kartik4091

#![allow(warnings)]

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::core::error::PdfError;

pub struct MonitorUtils {
    metrics: Arc<RwLock<HashMap<String, Metric>>>,
    config: MonitorConfig,
}

#[derive(Debug)]
pub struct Metric {
    name: String,
    value: f64,
    metric_type: MetricType,
    timestamp: chrono::DateTime<chrono::Utc>,
    labels: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
    Summary,
}

#[derive(Debug, Clone)]
pub struct MonitorConfig {
    pub enabled: bool,
    pub interval: std::time::Duration,
    pub max_metrics: usize,
}

impl MonitorUtils {
    pub fn new() -> Self {
        MonitorUtils {
            metrics: Arc::new(RwLock::new(HashMap::new())),
            config: MonitorConfig {
                enabled: true,
                interval: std::time::Duration::from_secs(60),
                max_metrics: 1000,
            },
        }
    }

    pub async fn record_metric(&mut self, name: &str, value: f64, metric_type: MetricType) -> Result<(), PdfError> {
        if !self.config.enabled {
            return Ok(());
        }

        let mut metrics = self.metrics.write().await;
        
        if metrics.len() >= self.config.max_metrics {
            // Remove oldest metric
            if let Some((k, _)) = metrics.iter().next() {
                metrics.remove(&k.to_string());
            }
        }

        metrics.insert(name.to_string(), Metric {
            name: name.to_string(),
            value,
            metric_type,
            timestamp: chrono::Utc::now(),
            labels: HashMap::new(),
        });

        Ok(())
    }

    pub async fn get_metric(&self, name: &str) -> Result<Option<Metric>, PdfError> {
        let metrics = self.metrics.read().await;
        Ok(metrics.get(name).cloned())
    }

    pub async fn get_metrics(&self) -> Result<Vec<Metric>, PdfError> {
        let metrics = self.metrics.read().await;
        Ok(metrics.values().cloned().collect())
    }

    pub async fn clear_metrics(&mut self) -> Result<(), PdfError> {
        let mut metrics = self.metrics.write().await;
        metrics.clear();
        Ok(())
    }
}

impl Clone for Metric {
    fn clone(&self) -> Self {
        Metric {
            name: self.name.clone(),
            value: self.value,
            metric_type: self.metric_type.clone(),
            timestamp: self.timestamp,
            labels: self.labels.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metric_recording() {
        let mut utils = MonitorUtils::new();
        utils.record_metric(
            "test",
            1.0,
            MetricType::Counter,
        ).await.unwrap();

        let metric = utils.get_metric("test").await.unwrap();
        assert!(metric.is_some());
    }
}
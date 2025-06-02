// Auto-patched by Alloma
// Timestamp: 2025-06-02 00:12:40
// User: kartik4091

#![allow(warnings)]

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};
use crate::core::error::PdfError;

pub struct MetricsManager {
    metrics: Arc<RwLock<MetricsData>>,
    config: MetricsConfig,
}

#[derive(Debug, Clone)]
pub struct MetricsData {
    pub counters: HashMap<String, u64>,
    pub gauges: HashMap<String, f64>,
    pub histograms: HashMap<String, Vec<f64>>,
    pub timers: HashMap<String, Vec<Duration>>,
}

#[derive(Debug, Clone)]
pub struct Duration {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub value: f64,
}

#[derive(Debug, Clone)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub collection_interval: std::time::Duration,
    pub max_samples: usize,
}

impl MetricsManager {
    pub fn new() -> Self {
        MetricsManager {
            metrics: Arc::new(RwLock::new(MetricsData {
                counters: HashMap::new(),
                gauges: HashMap::new(),
                histograms: HashMap::new(),
                timers: HashMap::new(),
            })),
            config: MetricsConfig {
                enabled: true,
                collection_interval: std::time::Duration::from_secs(60),
                max_samples: 1000,
            },
        }
    }

    pub async fn increment_counter(&mut self, name: &str, value: u64) -> Result<(), PdfError> {
        if !self.config.enabled {
            return Ok(());
        }

        let mut metrics = self.metrics.write().await;
        *metrics.counters.entry(name.to_string()).or_insert(0) += value;
        Ok(())
    }

    pub async fn set_gauge(&mut self, name: &str, value: f64) -> Result<(), PdfError> {
        if !self.config.enabled {
            return Ok(());
        }

        let mut metrics = self.metrics.write().await;
        metrics.gauges.insert(name.to_string(), value);
        Ok(())
    }

    pub async fn record_histogram(&mut self, name: &str, value: f64) -> Result<(), PdfError> {
        if !self.config.enabled {
            return Ok(());
        }

        let mut metrics = self.metrics.write().await;
        let values = metrics.histograms.entry(name.to_string()).or_insert_with(Vec::new);
        
        if values.len() >= self.config.max_samples {
            values.remove(0);
        }
        
        values.push(value);
        Ok(())
    }

    pub async fn start_timer(&mut self, name: &str) -> Result<(), PdfError> {
        if !self.config.enabled {
            return Ok(());
        }

        let mut metrics = self.metrics.write().await;
        let timers = metrics.timers.entry(name.to_string()).or_insert_with(Vec::new);
        
        timers.push(Duration {
            start: Utc::now(),
            end: Utc::now(),
            value: 0.0,
        });
        
        Ok(())
    }

    pub async fn stop_timer(&mut self, name: &str) -> Result<(), PdfError> {
        if !self.config.enabled {
            return Ok(());
        }

        let mut metrics = self.metrics.write().await;
        if let Some(timers) = metrics.timers.get_mut(name) {
            if let Some(timer) = timers.last_mut() {
                timer.end = Utc::now();
                timer.value = (timer.end - timer.start).num_milliseconds() as f64;
            }
        }
        Ok(())
    }

    pub async fn get_metrics(&self) -> Result<MetricsData, PdfError> {
        let metrics = self.metrics.read().await;
        Ok(metrics.clone())
    }

    pub async fn clear_metrics(&mut self) -> Result<(), PdfError> {
        let mut metrics = self.metrics.write().await;
        metrics.counters.clear();
        metrics.gauges.clear();
        metrics.histograms.clear();
        metrics.timers.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_recording() {
        let mut manager = MetricsManager::new();
        manager.increment_counter("test_counter", 1).await.unwrap();
        manager.set_gauge("test_gauge", 42.0).await.unwrap();
        
        let metrics = manager.get_metrics().await.unwrap();
        assert_eq!(*metrics.counters.get("test_counter").unwrap(), 1);
        assert_eq!(*metrics.gauges.get("test_gauge").unwrap(), 42.0);
    }
}
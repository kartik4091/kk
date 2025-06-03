//! Metrics Implementation
//! Author: kartik4091
//! Created: 2025-06-03 09:16:08 UTC

use super::*;
use std::{
    sync::Arc,
    time::{Duration, Instant},
    collections::{HashMap, BTreeMap},
};
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Enable metrics collection
    pub enabled: bool,
    /// Metrics retention period
    pub retention_period: Duration,
    /// Sample rate (0.0 - 1.0)
    pub sample_rate: f64,
    /// Maximum metrics to store
    pub max_metrics: usize,
}

/// Metric type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricType {
    /// Counter (monotonically increasing)
    Counter,
    /// Gauge (can go up or down)
    Gauge,
    /// Histogram (distribution of values)
    Histogram,
    /// Timer (duration measurements)
    Timer,
}

/// Metric value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricValue {
    /// Counter value
    Counter(u64),
    /// Gauge value
    Gauge(f64),
    /// Histogram values
    Histogram(Vec<f64>),
    /// Timer duration
    Timer(Duration),
}

/// Metric data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricPoint {
    /// Timestamp
    timestamp: chrono::DateTime<chrono::Utc>,
    /// Metric type
    metric_type: MetricType,
    /// Metric value
    value: MetricValue,
    /// Labels
    labels: HashMap<String, String>,
}

/// Metrics snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Metrics
    pub metrics: HashMap<String, Vec<MetricPoint>>,
    /// Statistics
    pub stats: MetricsStats,
}

/// Metrics statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricsStats {
    /// Total metrics collected
    pub total_metrics: u64,
    /// Active metrics
    pub active_metrics: u64,
    /// Memory usage
    pub memory_usage: usize,
    /// Collection time
    pub collection_time: Duration,
}

pub struct Metrics {
    /// Metrics configuration
    config: Arc<MetricsConfig>,
    /// Metrics storage
    metrics: Arc<RwLock<BTreeMap<String, Vec<MetricPoint>>>>,
    /// Statistics
    stats: Arc<RwLock<MetricsStats>>,
}

impl Metrics {
    /// Creates a new metrics instance
    pub fn new() -> Self {
        Self::with_config(MetricsConfig::default())
    }

    /// Creates a new metrics instance with configuration
    pub fn with_config(config: MetricsConfig) -> Self {
        Self {
            config: Arc::new(config),
            metrics: Arc::new(RwLock::new(BTreeMap::new())),
            stats: Arc::new(RwLock::new(MetricsStats::default())),
        }
    }

    /// Records an operation duration
    #[instrument(skip(self))]
    pub async fn record_operation(&self, name: &str, duration: Duration) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Apply sampling
        if rand::random::<f64>() > self.config.sample_rate {
            return Ok(());
        }

        let point = MetricPoint {
            timestamp: chrono::Utc::now(),
            metric_type: MetricType::Timer,
            value: MetricValue::Timer(duration),
            labels: HashMap::new(),
        };

        self.store_metric(name, point).await
    }

    /// Increments a counter
    #[instrument(skip(self))]
    pub async fn increment_counter(&self, name: &str, value: u64) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let point = MetricPoint {
            timestamp: chrono::Utc::now(),
            metric_type: MetricType::Counter,
            value: MetricValue::Counter(value),
            labels: HashMap::new(),
        };

        self.store_metric(name, point).await
    }

    /// Sets a gauge value
    #[instrument(skip(self))]
    pub async fn set_gauge(&self, name: &str, value: f64) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let point = MetricPoint {
            timestamp: chrono::Utc::now(),
            metric_type: MetricType::Gauge,
            value: MetricValue::Gauge(value),
            labels: HashMap::new(),
        };

        self.store_metric(name, point).await
    }

    /// Records a histogram value
    #[instrument(skip(self))]
    pub async fn record_histogram(&self, name: &str, value: f64) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let point = MetricPoint {
            timestamp: chrono::Utc::now(),
            metric_type: MetricType::Histogram,
            value: MetricValue::Histogram(vec![value]),
            labels: HashMap::new(),
        };

        self.store_metric(name, point).await
    }

    /// Stores a metric point
    #[instrument(skip(self, point))]
    async fn store_metric(&self, name: &str, point: MetricPoint) -> Result<()> {
        let mut metrics = self.metrics.write().await;
        let points = metrics.entry(name.to_string()).or_insert_with(Vec::new);

        // Enforce maximum metrics
        if points.len() >= self.config.max_metrics {
            points.remove(0);
        }

        points.push(point);

        // Update statistics
        let mut stats = self.stats.write().await;
        stats.total_metrics += 1;
        stats.active_metrics = metrics.len() as u64;
        stats.memory_usage = std::mem::size_of_val(&*metrics);

        Ok(())
    }

    /// Takes a snapshot of current metrics
    #[instrument(skip(self))]
    pub async fn take_snapshot(&self) -> Result<MetricsSnapshot> {
        let start = Instant::now();
        let metrics = self.metrics.read().await;
        let stats = self.stats.read().await;

        let snapshot = MetricsSnapshot {
            timestamp: chrono::Utc::now(),
            metrics: metrics.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            stats: MetricsStats {
                collection_time: start.elapsed(),
                ..(*stats).clone()
            },
        };

        Ok(snapshot)
    }

    /// Cleans up old metrics
    #[instrument(skip(self))]
    pub async fn cleanup(&self) -> Result<()> {
        let mut metrics = self.metrics.write().await;
        let cutoff = chrono::Utc::now() - chrono::Duration::from_std(self.config.retention_period)
            .map_err(|e| UtilError::Metric(format!("Invalid duration: {}", e)))?;

        for points in metrics.values_mut() {
            points.retain(|p| p.timestamp > cutoff);
        }

        // Remove empty metrics
        metrics.retain(|_, points| !points.is_empty());

        // Update statistics
        let mut stats = self.stats.write().await;
        stats.active_metrics = metrics.len() as u64;
        stats.memory_usage = std::mem::size_of_val(&*metrics);

        Ok(())
    }

    /// Gets statistics for a metric
    #[instrument(skip(self))]
    pub async fn get_stats(&self, name: &str) -> Result<Option<MetricStats>> {
        let metrics = self.metrics.read().await;
        
        if let Some(points) = metrics.get(name) {
            let mut stats = MetricStats::default();
            
            for point in points {
                match &point.value {
                    MetricValue::Counter(v) => stats.count += *v,
                    MetricValue::Gauge(v) => {
                        stats.last_value = *v;
                        stats.sum += *v;
                    }
                    MetricValue::Histogram(values) => {
                        for v in values {
                            stats.sum += *v;
                            stats.count += 1;
                        }
                    }
                    MetricValue::Timer(d) => {
                        stats.sum += d.as_secs_f64();
                        stats.count += 1;
                    }
                }
            }

            if stats.count > 0 {
                stats.average = stats.sum / stats.count as f64;
            }

            Ok(Some(stats))
        } else {
            Ok(None)
        }
    }
}

/// Metric statistics
#[derive(Debug, Clone, Default)]
pub struct MetricStats {
    /// Count of values
    pub count: u64,
    /// Sum of values
    pub sum: f64,
    /// Average value
    pub average: f64,
    /// Last value (for gauges)
    pub last_value: f64,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            retention_period: Duration::from_secs(3600), // 1 hour
            sample_rate: 1.0,
            max_metrics: 1000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_counter_metrics() {
        let metrics = Metrics::new();
        
        metrics.increment_counter("test_counter", 1).await.unwrap();
        metrics.increment_counter("test_counter", 2).await.unwrap();
        
        let stats = metrics.get_stats("test_counter").await.unwrap().unwrap();
        assert_eq!(stats.count, 3);
    }

    #[tokio::test]
    async fn test_gauge_metrics() {
        let metrics = Metrics::new();
        
        metrics.set_gauge("test_gauge", 1.5).await.unwrap();
        metrics.set_gauge("test_gauge", 2.5).await.unwrap();
        
        let stats = metrics.get_stats("test_gauge").await.unwrap().unwrap();
        assert_eq!(stats.last_value, 2.5);
    }

    #[tokio::test]
    async fn test_histogram_metrics() {
        let metrics = Metrics::new();
        
        metrics.record_histogram("test_hist", 1.0).await.unwrap();
        metrics.record_histogram("test_hist", 2.0).await.unwrap();
        metrics.record_histogram("test_hist", 3.0).await.unwrap();
        
        let stats = metrics.get_stats("test_hist").await.unwrap().unwrap();
        assert_eq!(stats.average, 2.0);
    }

    #[tokio::test]
    async fn test_operation_timing() {
        let metrics = Metrics::new();
        let duration = Duration::from_secs(1);
        
        metrics.record_operation("test_op", duration).await.unwrap();
        
        let stats = metrics.get_stats("test_op").await.unwrap().unwrap();
        assert_eq!(stats.sum, duration.as_secs_f64());
    }

    #[tokio::test]
    async fn test_metrics_cleanup() {
        let metrics = Metrics::with_config(MetricsConfig {
            retention_period: Duration::from_secs(0),
            ..MetricsConfig::default()
        });
        
        metrics.increment_counter("test_counter", 1).await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        metrics.cleanup().await.unwrap();
        
        let stats = metrics.get_stats("test_counter").await.unwrap();
        assert!(stats.is_none());
    }

    #[tokio::test]
    async fn test_metrics_snapshot() {
        let metrics = Metrics::new();
        
        metrics.increment_counter("test_counter", 1).await.unwrap();
        metrics.set_gauge("test_gauge", 1.5).await.unwrap();
        
        let snapshot = metrics.take_snapshot().await.unwrap();
        assert_eq!(snapshot.metrics.len(), 2);
        assert!(snapshot.stats.total_metrics > 0);
    }

    #[tokio::test]
    async fn test_sampling() {
        let metrics = Metrics::with_config(MetricsConfig {
            sample_rate: 0.0,
            ..MetricsConfig::default()
        });
        
        metrics.increment_counter("test_counter", 1).await.unwrap();
        
        let stats = metrics.get_stats("test_counter").await.unwrap();
        assert!(stats.is_none());
    }
          }

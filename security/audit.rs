use crate::{PdfError, SecurityConfig};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    sync::{Arc, RwLock},
};
use uuid::Uuid;

pub struct AuditSystem {
    state: Arc<RwLock<AuditState>>,
    config: AuditConfig,
}

struct AuditState {
    events: VecDeque<AuditEvent>,
    total_events: u64,
    last_event: Option<DateTime<Utc>>,
}

#[derive(Clone)]
struct AuditConfig {
    retention_period: std::time::Duration,
    max_events: usize,
    log_level: LogLevel,
}

#[derive(Clone, Copy, PartialEq)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    id: String,
    timestamp: DateTime<Utc>,
    event_type: EventType,
    user_id: String,
    resource_id: String,
    action: String,
    status: EventStatus,
    details: String,
    metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventType {
    Access,
    Security,
    Processing,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventStatus {
    Success,
    Failure,
    Warning,
}

impl AuditSystem {
    pub async fn new(security_config: &SecurityConfig) -> Result<Self, PdfError> {
        Ok(Self {
            state: Arc::new(RwLock::new(AuditState {
                events: VecDeque::with_capacity(1000),
                total_events: 0,
                last_event: None,
            })),
            config: AuditConfig::default(),
        })
    }

    pub async fn log_security_check(
        &self,
        data: &[u8],
        violations: &[super::SecurityViolation],
    ) -> Result<(), PdfError> {
        let event = AuditEvent {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: EventType::Security,
            user_id: "kartik4091".to_string(), // In a real system, get from context
            resource_id: Uuid::new_v4().to_string(), // Document ID
            action: "security_check".to_string(),
            status: if violations.is_empty() {
                EventStatus::Success
            } else {
                EventStatus::Failure
            },
            details: if violations.is_empty() {
                "Security check passed".to_string()
            } else {
                format!("Security violations found: {}", violations.len())
            },
            metadata: serde_json::json!({
                "document_size": data.len(),
                "violations": violations.len(),
                "check_time": Utc::now().to_rfc3339(),
            }),
        };

        self.log_event(event).await
    }

    async fn log_event(&self, event: AuditEvent) -> Result<(), PdfError> {
        let mut state = self.state.write().map_err(|_| 
            PdfError::Security("Failed to acquire state lock".to_string()))?;

        // Enforce max events limit
        while state.events.len() >= self.config.max_events {
            state.events.pop_front();
        }

        // Add new event
        state.events.push_back(event);
        state.total_events += 1;
        state.last_event = Some(Utc::now());

        // Clean up old events
        self.cleanup_old_events(&mut state)?;

        Ok(())
    }

    fn cleanup_old_events(&self, state: &mut AuditState) -> Result<(), PdfError> {
        let cutoff = Utc::now() - self.config.retention_period;
        while let Some(event) = state.events.front() {
            if event.timestamp > cutoff {
                break;
            }
            state.events.pop_front();
        }
        Ok(())
    }

    pub async fn get_events(
        &self,
        filter: Option<EventFilter>,
    ) -> Result<Vec<AuditEvent>, PdfError> {
        let state = self.state.read().map_err(|_| 
            PdfError::Security("Failed to acquire state lock".to_string()))?;

        let events: Vec<AuditEvent> = if let Some(filter) = filter {
            state.events
                .iter()
                .filter(|event| Self::matches_filter(event, &filter))
                .cloned()
                .collect()
        } else {
            state.events.iter().cloned().collect()
        };

        Ok(events)
    }

    fn matches_filter(event: &AuditEvent, filter: &EventFilter) -> bool {
        if let Some(ref event_type) = filter.event_type {
            if !matches!(&event.event_type, event_type) {
                return false;
            }
        }

        if let Some(ref status) = filter.status {
            if !matches!(&event.status, status) {
                return false;
            }
        }

        if let Some(start_time) = filter.start_time {
            if event.timestamp < start_time {
                return false;
            }
        }

        if let Some(end_time) = filter.end_time {
            if event.timestamp > end_time {
                return false;
            }
        }

        true
    }
}

#[derive(Debug, Clone)]
pub struct EventFilter {
    pub event_type: Option<EventType>,
    pub status: Option<EventStatus>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            retention_period: std::time::Duration::from_secs(30 * 24 * 60 * 60), // 30 days
            max_events: 10000,
            log_level: LogLevel::Info,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_audit_system_creation() {
        let config = SecurityConfig::default();
        let system = AuditSystem::new(&config).await;
        assert!(system.is_ok());
    }

    #[tokio::test]
    async fn test_event_logging() {
        let config = SecurityConfig::default();
        let system = AuditSystem::new(&config).await.unwrap();
        
        let sample_data = b"Test document data";
        let result = system.log_security_check(sample_data, &[]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_event_retrieval() {
        let config = SecurityConfig::default();
        let system = AuditSystem::new(&config).await.unwrap();
        
        // Log some events
        let sample_data = b"Test document data";
        system.log_security_check(sample_data, &[]).await.unwrap();
        
        // Retrieve events
        let events = system.get_events(None).await.unwrap();
        assert!(!events.is_empty());
    }
}
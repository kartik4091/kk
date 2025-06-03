use crate::{PdfError, SecurityConfig};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};
use uuid::Uuid;

pub struct PolicyEngine {
    state: Arc<RwLock<PolicyState>>,
    config: PolicyConfig,
    policies: Arc<RwLock<HashMap<String, Policy>>>,
    enforcer: PolicyEnforcer,
}

struct PolicyState {
    evaluations_performed: u64,
    last_evaluation: Option<DateTime<Utc>>,
    active_evaluations: u32,
    cached_decisions: HashMap<String, CachedDecision>,
}

#[derive(Clone)]
struct PolicyConfig {
    cache_ttl: std::time::Duration,
    max_cached_decisions: usize,
    strict_mode: bool,
    default_effect: Effect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    id: String,
    name: String,
    description: String,
    version: String,
    rules: Vec<PolicyRule>,
    metadata: PolicyMetadata,
    status: PolicyStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    id: String,
    condition: PolicyCondition,
    effect: Effect,
    resources: Vec<String>,
    actions: Vec<String>,
    principals: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyCondition {
    Always,
    Never,
    TimeRange {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
    IpRange {
        allowed_ranges: Vec<String>,
    },
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Effect {
    Allow,
    Deny,
    RequireMFA,
    RequireApproval,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyMetadata {
    created_at: DateTime<Utc>,
    created_by: String,
    updated_at: DateTime<Utc>,
    updated_by: String,
    version: u32,
    tags: HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyStatus {
    Active,
    Inactive,
    Deprecated,
    PendingApproval,
}

struct CachedDecision {
    effect: Effect,
    timestamp: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

pub struct PolicyEnforcer {
    validation_rules: ValidationRules,
    compliance_checker: ComplianceChecker,
}

struct ValidationRules {
    rules: Vec<ValidationRule>,
}

struct ValidationRule {
    id: String,
    name: String,
    validator: Box<dyn Fn(&Policy) -> bool + Send + Sync>,
}

struct ComplianceChecker {
    checks: Vec<ComplianceCheck>,
}

struct ComplianceCheck {
    id: String,
    name: String,
    checker: Box<dyn Fn(&Policy) -> bool + Send + Sync>,
}

impl PolicyEngine {
    pub async fn new(security_config: &SecurityConfig) -> Result<Self, PdfError> {
        let current_time = Utc::parse_from_str("2025-06-02 18:35:13", "%Y-%m-%d %H:%M:%S")
            .map_err(|_| PdfError::Security("Invalid current time".to_string()))?;

        Ok(Self {
            state: Arc::new(RwLock::new(PolicyState {
                evaluations_performed: 0,
                last_evaluation: None,
                active_evaluations: 0,
                cached_decisions: HashMap::new(),
            })),
            config: PolicyConfig::default(),
            policies: Arc::new(RwLock::new(Self::initialize_policies(current_time)?)),
            enforcer: PolicyEnforcer::new(),
        })
    }

    fn initialize_policies(current_time: DateTime<Utc>) -> Result<HashMap<String, Policy>, PdfError> {
        let mut policies = HashMap::new();

        // Default document access policy
        let policy_id = Uuid::new_v4().to_string();
        policies.insert(policy_id.clone(), Policy {
            id: policy_id,
            name: "Default Document Access Policy".to_string(),
            description: "Controls default access to PDF documents".to_string(),
            version: "1.0.0".to_string(),
            rules: vec![
                PolicyRule {
                    id: Uuid::new_v4().to_string(),
                    condition: PolicyCondition::Always,
                    effect: Effect::Allow,
                    resources: vec!["document/*".to_string()],
                    actions: vec!["read".to_string(), "write".to_string()],
                    principals: vec!["authenticated-users".to_string()],
                },
                PolicyRule {
                    id: Uuid::new_v4().to_string(),
                    condition: PolicyCondition::TimeRange {
                        start: current_time,
                        end: current_time + chrono::Duration::days(365),
                    },
                    effect: Effect::RequireMFA,
                    resources: vec!["document/sensitive/*".to_string()],
                    actions: vec!["modify".to_string(), "delete".to_string()],
                    principals: vec!["admin-users".to_string()],
                },
            ],
            metadata: PolicyMetadata {
                created_at: current_time,
                created_by: "kartik4091".to_string(),
                updated_at: current_time,
                updated_by: "kartik4091".to_string(),
                version: 1,
                tags: vec!["document".to_string(), "access-control".to_string()].into_iter().collect(),
            },
            status: PolicyStatus::Active,
        });

        Ok(policies)
    }

    pub async fn evaluate_policy(
        &self,
        resource: &str,
        action: &str,
        principal: &str,
    ) -> Result<Effect, PdfError> {
        let mut state = self.state.write().map_err(|_| 
            PdfError::Security("Failed to acquire state lock".to_string()))?;
        
        state.active_evaluations += 1;

        // Check cache first
        let cache_key = format!("{}:{}:{}", resource, action, principal);
        if let Some(cached) = state.cached_decisions.get(&cache_key) {
            if cached.expires_at > Utc::now() {
                return Ok(cached.effect.clone());
            }
            state.cached_decisions.remove(&cache_key);
        }

        let result = self.internal_evaluate_policy(resource, action, principal).await;

        // Update state and cache
        state.evaluations_performed += 1;
        state.last_evaluation = Some(Utc::now());
        state.active_evaluations -= 1;

        if let Ok(effect) = &result {
            state.cached_decisions.insert(cache_key, CachedDecision {
                effect: effect.clone(),
                timestamp: Utc::now(),
                expires_at: Utc::now() + self.config.cache_ttl,
            });
        }

        result
    }

    async fn internal_evaluate_policy(
        &self,
        resource: &str,
        action: &str,
        principal: &str,
    ) -> Result<Effect, PdfError> {
        let policies = self.policies.read().map_err(|_| 
            PdfError::Security("Failed to acquire policies lock".to_string()))?;

        let current_time = Utc::parse_from_str("2025-06-02 18:35:13", "%Y-%m-%d %H:%M:%S")
            .map_err(|_| PdfError::Security("Invalid current time".to_string()))?;

        let mut final_effect = self.config.default_effect.clone();

        for policy in policies.values() {
            if !matches!(policy.status, PolicyStatus::Active) {
                continue;
            }

            for rule in &policy.rules {
                if !self.matches_resource(&rule.resources, resource) ||
                   !rule.actions.contains(&action.to_string()) ||
                   !rule.principals.contains(&principal.to_string()) {
                    continue;
                }

                if self.evaluate_condition(&rule.condition, current_time) {
                    match rule.effect {
                        Effect::Deny => return Ok(Effect::Deny), // Explicit deny takes precedence
                        Effect::RequireMFA => final_effect = Effect::RequireMFA,
                        Effect::RequireApproval => {
                            if matches!(final_effect, Effect::Allow) {
                                final_effect = Effect::RequireApproval;
                            }
                        }
                        Effect::Allow => {
                            if matches!(final_effect, Effect::Allow) {
                                final_effect = Effect::Allow;
                            }
                        }
                    }
                }
            }
        }

        Ok(final_effect)
    }

    fn matches_resource(&self, policy_resources: &[String], resource: &str) -> bool {
        policy_resources.iter().any(|pattern| {
            if pattern.ends_with("*") {
                resource.starts_with(&pattern[..pattern.len() - 1])
            } else {
                pattern == resource
            }
        })
    }

    fn evaluate_condition(&self, condition: &PolicyCondition, current_time: DateTime<Utc>) -> bool {
        match condition {
            PolicyCondition::Always => true,
            PolicyCondition::Never => false,
            PolicyCondition::TimeRange { start, end } => {
                current_time >= *start && current_time <= *end
            }
            PolicyCondition::IpRange { allowed_ranges } => {
                // In a real implementation, this would check actual IP ranges
                true
            }
            PolicyCondition::Custom(expr) => {
                // In a real implementation, this would evaluate custom expressions
                true
            }
        }
    }
}

impl PolicyEnforcer {
    fn new() -> Self {
        Self {
            validation_rules: ValidationRules::new(),
            compliance_checker: ComplianceChecker::new(),
        }
    }
}

impl ValidationRules {
    fn new() -> Self {
        Self {
            rules: vec![
                ValidationRule {
                    id: Uuid::new_v4().to_string(),
                    name: "Resource Pattern Validation".to_string(),
                    validator: Box::new(|policy| {
                        policy.rules.iter().all(|rule| 
                            rule.resources.iter().all(|r| !r.is_empty())
                        )
                    }),
                },
            ],
        }
    }
}

impl ComplianceChecker {
    fn new() -> Self {
        Self {
            checks: vec![
                ComplianceCheck {
                    id: Uuid::new_v4().to_string(),
                    name: "Least Privilege Check".to_string(),
                    checker: Box::new(|policy| {
                        !policy.rules.iter().any(|rule| 
                            rule.resources.contains(&"*".to_string()) && 
                            matches!(rule.effect, Effect::Allow)
                        )
                    }),
                },
            ],
        }
    }
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            cache_ttl: std::time::Duration::from_secs(300), // 5 minutes
            max_cached_decisions: 10000,
            strict_mode: true,
            default_effect: Effect::Deny,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_policy_engine_creation() {
        let config = SecurityConfig::default();
        let engine = PolicyEngine::new(&config).await;
        assert!(engine.is_ok());
    }

    #[tokio::test]
    async fn test_policy_evaluation() {
        let config = SecurityConfig::default();
        let engine = PolicyEngine::new(&config).await.unwrap();
        
        let result = engine.evaluate_policy(
            "document/test.pdf",
            "read",
            "authenticated-users",
        ).await;
        
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Effect::Allow));
    }

    #[tokio::test]
    async fn test_sensitive_document_policy() {
        let config = SecurityConfig::default();
        let engine = PolicyEngine::new(&config).await.unwrap();
        
        let result = engine.evaluate_policy(
            "document/sensitive/secret.pdf",
            "modify",
            "admin-users",
        ).await;
        
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Effect::RequireMFA));
    }
}
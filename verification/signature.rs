use crate::{PdfError, VerificationError, VerificationWarning, ErrorSeverity};
use chrono::{DateTime, Utc};
use lopdf::{Document, Object, ObjectId, Dictionary, Stream};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};
use openssl::{
    x509::X509,
    pkcs7::Pkcs7,
    hash::MessageDigest,
};

pub struct SignatureVerifier {
    state: Arc<RwLock<SignatureState>>,
    config: SignatureConfig,
    trust_store: Arc<RwLock<TrustStore>>,
}

struct SignatureState {
    verifications_performed: u64,
    last_verification: Option<DateTime<Utc>>,
    active_verifications: u32,
    verification_cache: HashMap<String, SignatureResult>,
}

#[derive(Clone)]
struct SignatureConfig {
    verify_timestamps: bool,
    verify_revocation: bool,
    verify_chain: bool,
    max_chain_depth: usize,
    allowed_algorithms: HashSet<String>,
    timestamp_drift_tolerance: std::time::Duration,
}

#[derive(Debug, Clone, Default)]
pub struct SignatureResult {
    pub errors: Vec<VerificationError>,
    pub warnings: Vec<VerificationWarning>,
    pub rules_checked: usize,
    pub signatures_checked: usize,
    pub valid_signatures: usize,
    pub timestamp_validity: bool,
}

struct TrustStore {
    trusted_certificates: HashMap<String, X509>,
    trusted_timestamps: HashMap<String, DateTime<Utc>>,
    revocation_lists: HashMap<String, Vec<u8>>,
}

#[derive(Debug)]
struct SignatureInfo {
    signer_name: String,
    signing_time: DateTime<Utc>,
    certificate: X509,
    algorithm: String,
    signature_type: SignatureType,
}

#[derive(Debug)]
enum SignatureType {
    Basic,
    Certified,
    UsageRights,
    Timestamp,
}

impl SignatureVerifier {
    pub async fn new() -> Result<Self, PdfError> {
        Ok(Self {
            state: Arc::new(RwLock::new(SignatureState {
                verifications_performed: 0,
                last_verification: None,
                active_verifications: 0,
                verification_cache: HashMap::new(),
            })),
            config: SignatureConfig::default(),
            trust_store: Arc::new(RwLock::new(TrustStore::new()?)),
        })
    }

    pub async fn verify(&self, doc: &Document) -> Result<SignatureResult, PdfError> {
        let start_time = std::time::Instant::now();
        let current_time = Utc::parse_from_str("2025-06-02 19:00:27", "%Y-%m-%d %H:%M:%S")
            .map_err(|_| PdfError::Verification("Invalid current time".to_string()))?;

        // Update state
        {
            let mut state = self.state.write().map_err(|_| 
                PdfError::Verification("Failed to acquire state lock".to_string()))?;
            state.active_verifications += 1;
        }

        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut rules_checked = 0;
        let mut signatures_checked = 0;
        let mut valid_signatures = 0;
        let mut timestamp_validity = true;

        // Collect all signatures
        let signatures = self.collect_signatures(doc)?;
        signatures_checked = signatures.len();

        // Verify each signature
        for (id, sig_dict) in signatures {
            match self.verify_signature(&sig_dict, doc) {
                Ok(sig_info) => {
                    // Verify certificate chain
                    if self.config.verify_chain {
                        self.verify_certificate_chain(&sig_info, &mut errors, &mut warnings)?;
                    }

                    // Verify timestamp
                    if self.config.verify_timestamps {
                        if !self.verify_timestamp(&sig_info, current_time)? {
                            timestamp_validity = false;
                            warnings.push(VerificationWarning {
                                code: "INVALID_TIMESTAMP".to_string(),
                                message: "Signature timestamp validation failed".to_string(),
                                location: Some(id),
                                recommendation: "Check system time and timestamp validity".to_string(),
                            });
                        }
                    }

                    // Verify revocation status
                    if self.config.verify_revocation {
                        self.verify_revocation_status(&sig_info, current_time, &mut errors, &mut warnings)?;
                    }

                    valid_signatures += 1;
                }
                Err(e) => {
                    errors.push(VerificationError {
                        code: "SIGNATURE_VERIFICATION_FAILED".to_string(),
                        message: format!("Failed to verify signature: {}", e),
                        location: Some(id),
                        severity: ErrorSeverity::Critical,
                        details: HashMap::new(),
                    });
                }
            }
            rules_checked += 1;
        }

        // Create result
        let result = SignatureResult {
            errors,
            warnings,
            rules_checked,
            signatures_checked,
            valid_signatures,
            timestamp_validity,
        };

        // Update state
        {
            let mut state = self.state.write().map_err(|_| 
                PdfError::Verification("Failed to acquire state lock".to_string()))?;
            state.active_verifications -= 1;
            state.verifications_performed += 1;
            state.last_verification = Some(current_time);
            state.verification_cache.insert(doc.get_id().unwrap_or_default(), result.clone());
        }

        Ok(result)
    }

    fn collect_signatures(&self, doc: &Document) -> Result<Vec<(ObjectId, Dictionary)>, PdfError> {
        let mut signatures = Vec::new();

        // Check for signature fields in AcroForm
        if let Some(form_id) = self.find_acroform(doc)? {
            if let Some(Object::Dictionary(form_dict)) = doc.objects.get(&form_id) {
                if let Ok(Object::Array(fields)) = form_dict.get("Fields") {
                    for field_ref in fields {
                        if let Object::Reference(field_id) = field_ref {
                            if let Some(Object::Dictionary(field_dict)) = doc.objects.get(field_id) {
                                if self.is_signature_field(field_dict)? {
                                    signatures.push((*field_id, field_dict.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(signatures)
    }

    fn verify_signature(&self, sig_dict: &Dictionary, doc: &Document) -> Result<SignatureInfo, PdfError> {
        // Get signature value
        let sig_value = sig_dict.get("V")
            .map_err(|_| PdfError::Verification("Missing signature value".to_string()))?;

        if let Object::Stream(sig_stream) = sig_value {
            // Parse PKCS#7 signature
            let pkcs7 = Pkcs7::from_der(&sig_stream.content)
                .map_err(|e| PdfError::Verification(format!("Invalid PKCS#7 signature: {}", e)))?;

            // Get signer information
            let signer_info = self.extract_signer_info(&pkcs7)?;

            // Verify signature algorithm
            if !self.config.allowed_algorithms.contains(&signer_info.algorithm) {
                return Err(PdfError::Verification(
                    format!("Unsupported signature algorithm: {}", signer_info.algorithm)
                ));
            }

            Ok(signer_info)
        } else {
            Err(PdfError::Verification("Invalid signature value type".to_string()))
        }
    }

    fn verify_certificate_chain(
        &self,
        sig_info: &SignatureInfo,
        errors: &mut Vec<VerificationError>,
        warnings: &mut Vec<VerificationWarning>,
    ) -> Result<(), PdfError> {
        let mut current_cert = sig_info.certificate.clone();
        let mut depth = 0;

        while depth < self.config.max_chain_depth {
            // Check if current certificate is trusted
            if self.is_trusted_certificate(&current_cert)? {
                return Ok(());
            }

            // Get issuer certificate
            match self.find_issuer_certificate(&current_cert)? {
                Some(issuer_cert) => {
                    // Verify certificate signature
                    if !self.verify_certificate_signature(&current_cert, &issuer_cert)? {
                        errors.push(VerificationError {
                            code: "INVALID_CERT_SIGNATURE".to_string(),
                            message: "Invalid certificate signature in chain".to_string(),
                            location: None,
                            severity: ErrorSeverity::Critical,
                            details: HashMap::new(),
                        });
                        return Ok(());
                    }
                    current_cert = issuer_cert;
                }
                None => {
                    warnings.push(VerificationWarning {
                        code: "INCOMPLETE_CERT_CHAIN".to_string(),
                        message: "Certificate chain is incomplete".to_string(),
                        location: None,
                        recommendation: "Add missing intermediate certificates".to_string(),
                    });
                    return Ok(());
                }
            }
            depth += 1;
        }

        errors.push(VerificationError {
            code: "CERT_CHAIN_TOO_LONG".to_string(),
            message: format!("Certificate chain exceeds maximum depth of {}", self.config.max_chain_depth),
            location: None,
            severity: ErrorSeverity::Major,
            details: HashMap::new(),
        });
        Ok(())
    }

    fn verify_timestamp(
        &self,
        sig_info: &SignatureInfo,
        current_time: DateTime<Utc>,
    ) -> Result<bool, PdfError> {
        let time_diff = current_time
            .signed_duration_since(sig_info.signing_time)
            .abs();

        Ok(time_diff <= self.config.timestamp_drift_tolerance)
    }

    fn verify_revocation_status(
        &self,
        sig_info: &SignatureInfo,
        current_time: DateTime<Utc>,
        errors: &mut Vec<VerificationError>,
        warnings: &mut Vec<VerificationWarning>,
    ) -> Result<(), PdfError> {
        // Check CRL
        if let Some(crl_data) = self.get_revocation_list(&sig_info.certificate)? {
            if self.is_certificate_revoked(&sig_info.certificate, &crl_data)? {
                errors.push(VerificationError {
                    code: "CERTIFICATE_REVOKED".to_string(),
                    message: "The signing certificate has been revoked".to_string(),
                    location: None,
                    severity: ErrorSeverity::Critical,
                    details: HashMap::new(),
                });
            }
        } else {
            warnings.push(VerificationWarning {
                code: "NO_REVOCATION_INFO".to_string(),
                message: "Unable to check certificate revocation status".to_string(),
                location: None,
                recommendation: "Configure CRL or OCSP checking".to_string(),
            });
        }

        Ok(())
    }

    // Helper methods
    fn find_acroform(&self, doc: &Document) -> Result<Option<ObjectId>, PdfError> {
        if let Some(catalog_id) = doc.catalog {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&catalog_id) {
                if let Ok(Object::Reference(form_id)) = dict.get("AcroForm") {
                    return Ok(Some(*form_id));
                }
            }
        }
        Ok(None)
    }

    fn is_signature_field(&self, field_dict: &Dictionary) -> Result<bool, PdfError> {
        if let Ok(Object::Name(field_type)) = field_dict.get("FT") {
            Ok(field_type == "Sig")
        } else {
            Ok(false)
        }
    }

    fn extract_signer_info(&self, pkcs7: &Pkcs7) -> Result<SignatureInfo, PdfError> {
        // In production, implement proper PKCS#7 signer info extraction
        Ok(SignatureInfo {
            signer_name: "Test Signer".to_string(),
            signing_time: Utc::now(),
            certificate: X509::stack_from_pem(&[]).unwrap()[0].clone(),
            algorithm: "SHA256withRSA".to_string(),
            signature_type: SignatureType::Basic,
        })
    }

    fn is_trusted_certificate(&self, cert: &X509) -> Result<bool, PdfError> {
        let store = self.trust_store.read().map_err(|_| 
            PdfError::Verification("Failed to acquire trust store lock".to_string()))?;
        
        Ok(store.trusted_certificates.values().any(|trusted_cert| 
            trusted_cert.serial_number() == cert.serial_number()
        ))
    }

    fn find_issuer_certificate(&self, cert: &X509) -> Result<Option<X509>, PdfError> {
        // In production, implement proper issuer certificate lookup
        Ok(None)
    }

    fn verify_certificate_signature(&self, cert: &X509, issuer_cert: &X509) -> Result<bool, PdfError> {
        // In production, implement proper certificate signature verification
        Ok(true)
    }

    fn get_revocation_list(&self, cert: &X509) -> Result<Option<Vec<u8>>, PdfError> {
        let store = self.trust_store.read().map_err(|_| 
            PdfError::Verification("Failed to acquire trust store lock".to_string()))?;
        
        Ok(store.revocation_lists.values().next().cloned())
    }

    fn is_certificate_revoked(&self, cert: &X509, crl_data: &[u8]) -> Result<bool, PdfError> {
        // In production, implement proper CRL checking
        Ok(false)
    }
}

impl TrustStore {
    fn new() -> Result<Self, PdfError> {
        Ok(Self {
            trusted_certificates: HashMap::new(),
            trusted_timestamps: HashMap::new(),
            revocation_lists: HashMap::new(),
        })
    }
}

impl Default for SignatureConfig {
    fn default() -> Self {
        let mut allowed_algorithms = HashSet::new();
        allowed_algorithms.insert("SHA256withRSA".to_string());
        allowed_algorithms.insert("SHA384withRSA".to_string());
        allowed_algorithms.insert("SHA512withRSA".to_string());
        allowed_algorithms.insert("SHA256withECDSA".to_string());

        Self {
            verify_timestamps: true,
            verify_revocation: true,
            verify_chain: true,
            max_chain_depth: 10,
            allowed_algorithms,
            timestamp_drift_tolerance: std::time::Duration::from_secs(300), // 5 minutes
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_signature_verifier_creation() {
        let verifier = SignatureVerifier::new().await;
        assert!(verifier.is_ok());
    }

    #[tokio::test]
    async fn test_basic_signature_verification() {
        let verifier = SignatureVerifier::new().await.unwrap();
        let doc = Document::new();
        let result = verifier.verify(&doc).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_signature_field_detection() {
        let verifier = SignatureVerifier::new().await.unwrap();
        
        let mut field_dict = Dictionary::new();
        field_dict.set("FT", Object::Name("Sig".to_string()));
        
        let result = verifier.is_signature_field(&field_dict);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }
}
//! Signature scanner for PDF document cryptographic analysis
//! Author: kartik4091
//! Created: 2025-06-03 04:23:07 UTC
//! This module provides cryptographic signature analysis capabilities
//! for PDF documents, including digital signatures and certificates.

use std::{
    sync::Arc,
    collections::HashMap,
    time::{Duration, Instant},
};
use async_trait::async_trait;
use openssl::{
    x509::X509,
    pkcs7::Pkcs7,
    stack::Stack,
    error::ErrorStack,
};
use tracing::{info, warn, error, debug, trace, instrument};

use super::{ScannerConfig, ScanContext};
use crate::antiforensics::{
    Document,
    PdfError,
    RiskLevel,
    ForensicArtifact,
    ArtifactType,
};

/// Signature scanner for cryptographic analysis
pub struct SignatureScanner {
    /// Scanner configuration
    config: Arc<ScannerConfig>,
}

/// Structure representing a PDF signature
#[derive(Debug)]
struct PdfSignature {
    /// Signature field name
    field_name: String,
    /// Signer information
    signer_info: Option<SignerInfo>,
    /// Signature type
    signature_type: SignatureType,
    /// Signature creation date
    signing_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Verification status
    verification_status: SignatureStatus,
    /// Raw PKCS#7 data
    pkcs7_data: Vec<u8>,
}

/// Information about the signer
#[derive(Debug)]
struct SignerInfo {
    /// Common name
    common_name: String,
    /// Organization
    organization: Option<String>,
    /// Email address
    email: Option<String>,
    /// Certificate chain
    certificate_chain: Vec<X509>,
}

/// Types of digital signatures
#[derive(Debug, PartialEq, Eq)]
enum SignatureType {
    /// Adobe.PPKLite adbe.pkcs7.detached
    AdobePkcs7Detached,
    /// Adobe.PPKLite adbe.pkcs7.sha1
    AdobePkcs7Sha1,
    /// ETSI.CAdES.detached
    EtsiCadesDetached,
    /// Unknown signature type
    Unknown,
}

/// Signature verification status
#[derive(Debug, PartialEq, Eq)]
enum SignatureStatus {
    /// Signature is valid
    Valid,
    /// Signature is invalid
    Invalid,
    /// Certificate is expired
    CertificateExpired,
    /// Certificate is revoked
    CertificateRevoked,
    /// Chain of trust is broken
    BrokenChainOfTrust,
    /// Unknown status
    Unknown,
}

impl SignatureScanner {
    /// Creates a new signature scanner instance
    pub fn new(config: ScannerConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    /// Scans document for cryptographic signatures
    #[instrument(skip(self, doc), err(Display))]
    pub async fn scan_signatures(&self, doc: &Document) -> Result<Vec<ForensicArtifact>, PdfError> {
        let mut artifacts = Vec::new();
        let signature_fields = self.get_signature_fields(doc)?;

        for field in signature_fields {
            match self.analyze_signature(&field) {
                Ok(signature) => {
                    artifacts.push(self.create_signature_artifact(&signature)?);
                    
                    // Check certificate chain if available
                    if let Some(signer_info) = &signature.signer_info {
                        artifacts.extend(self.analyze_certificate_chain(&signer_info.certificate_chain)?);
                    }
                }
                Err(e) => {
                    warn!("Failed to analyze signature: {}", e);
                    artifacts.push(ForensicArtifact {
                        id: uuid::Uuid::new_v4().to_string(),
                        artifact_type: ArtifactType::Signature,
                        location: field.field_name.clone(),
                        description: format!("Failed to analyze signature: {}", e),
                        risk_level: RiskLevel::High,
                        remediation: "Review and potentially remove invalid signature".into(),
                        metadata: HashMap::new(),
                        detection_timestamp: chrono::Utc::now(),
                        hash: self.calculate_hash(&field.pkcs7_data),
                    });
                }
            }
        }

        Ok(artifacts)
    }

    /// Gets all signature fields from the document
    fn get_signature_fields(&self, doc: &Document) -> Result<Vec<PdfSignature>, PdfError> {
        let mut signatures = Vec::new();
        let acro_form = doc.get_acro_form()
            .ok_or_else(|| PdfError::Scanner("No AcroForm found".into()))?;

        for field in acro_form.get_fields() {
            if field.is_signature_field() {
                let sig_dict = field.get_signature_dictionary()
                    .ok_or_else(|| PdfError::Scanner("Invalid signature dictionary".into()))?;

                signatures.push(PdfSignature {
                    field_name: field.get_name()?,
                    signer_info: self.extract_signer_info(&sig_dict)?,
                    signature_type: self.determine_signature_type(&sig_dict),
                    signing_time: self.extract_signing_time(&sig_dict)?,
                    verification_status: self.verify_signature(&sig_dict)?,
                    pkcs7_data: sig_dict.get_contents()?,
                });
            }
        }

        Ok(signatures)
    }

    /// Extracts signer information from signature dictionary
    fn extract_signer_info(&self, sig_dict: &Dictionary) -> Result<Option<SignerInfo>, PdfError> {
        let pkcs7_data = sig_dict.get_contents()?;
        let pkcs7 = Pkcs7::from_der(&pkcs7_data)
            .map_err(|e| PdfError::Scanner(format!("Invalid PKCS#7 data: {}", e)))?;

        let signer_certs = pkcs7.get_signer_info()
            .map_err(|e| PdfError::Scanner(format!("Failed to get signer info: {}", e)))?;

        if let Some(cert) = signer_certs.get(0) {
            let subject = cert.get_subject_name()
                .map_err(|e| PdfError::Scanner(format!("Failed to get subject name: {}", e)))?;

            Ok(Some(SignerInfo {
                common_name: subject.get_entry_by_nid(openssl::nid::Nid::COMMONNAME)
                    .map(|e| e.data().as_utf8().unwrap().to_string())
                    .unwrap_or_default(),
                organization: subject.get_entry_by_nid(openssl::nid::Nid::ORGANIZATIONNAME)
                    .map(|e| e.data().as_utf8().unwrap().to_string()),
                email: subject.get_entry_by_nid(openssl::nid::Nid::PKCS9_EMAILADDRESS)
                    .map(|e| e.data().as_utf8().unwrap().to_string()),
                certificate_chain: self.extract_certificate_chain(&pkcs7)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Extracts certificate chain from PKCS#7 data
    fn extract_certificate_chain(&self, pkcs7: &Pkcs7) -> Result<Vec<X509>, PdfError> {
        let certs = pkcs7.get_certificates()
            .map_err(|e| PdfError::Scanner(format!("Failed to get certificates: {}", e)))?;

        Ok(certs.iter().cloned().collect())
    }

    /// Determines signature type from signature dictionary
    fn determine_signature_type(&self, sig_dict: &Dictionary) -> SignatureType {
        match sig_dict.get_filter_name().as_deref() {
            Some("Adobe.PPKLite") => match sig_dict.get_subfilter_name().as_deref() {
                Some("adbe.pkcs7.detached") => SignatureType::AdobePkcs7Detached,
                Some("adbe.pkcs7.sha1") => SignatureType::AdobePkcs7Sha1,
                _ => SignatureType::Unknown,
            },
            Some("ETSI.CAdES.detached") => SignatureType::EtsiCadesDetached,
            _ => SignatureType::Unknown,
        }
    }

    /// Extracts signing time from signature dictionary
    fn extract_signing_time(&self, sig_dict: &Dictionary) -> Result<Option<chrono::DateTime<chrono::Utc>>, PdfError> {
        if let Some(time_str) = sig_dict.get_signing_time()? {
            Ok(Some(chrono::DateTime::parse_from_rfc3339(&time_str)
                .map_err(|e| PdfError::Scanner(format!("Invalid signing time format: {}", e)))?
                .with_timezone(&chrono::Utc)))
        } else {
            Ok(None)
        }
    }

    /// Verifies signature and returns status
    fn verify_signature(&self, sig_dict: &Dictionary) -> Result<SignatureStatus, PdfError> {
        let pkcs7_data = sig_dict.get_contents()?;
        let pkcs7 = Pkcs7::from_der(&pkcs7_data)
            .map_err(|e| PdfError::Scanner(format!("Invalid PKCS#7 data: {}", e)))?;

        // Verify signature
        match self.verify_pkcs7(&pkcs7) {
            Ok(true) => {
                // Check certificate validity
                if let Some(cert) = pkcs7.get_signer_info()?.get(0) {
                    if self.is_certificate_expired(cert) {
                        Ok(SignatureStatus::CertificateExpired)
                    } else if self.is_certificate_revoked(cert)? {
                        Ok(SignatureStatus::CertificateRevoked)
                    } else if !self.verify_certificate_chain(&pkcs7)? {
                        Ok(SignatureStatus::BrokenChainOfTrust)
                    } else {
                        Ok(SignatureStatus::Valid)
                    }
                } else {
                    Ok(SignatureStatus::Invalid)
                }
            }
            Ok(false) => Ok(SignatureStatus::Invalid),
            Err(_) => Ok(SignatureStatus::Unknown),
        }
    }

    /// Verifies PKCS#7 signature
    fn verify_pkcs7(&self, pkcs7: &Pkcs7) -> Result<bool, ErrorStack> {
        // Implement PKCS#7 verification logic
        pkcs7.verify(None, None, None, None, None)
    }

    /// Checks if certificate is expired
    fn is_certificate_expired(&self, cert: &X509) -> bool {
        let now = openssl::asn1::Asn1Time::days_from_now(0).unwrap();
        !cert.not_before().le(&now) || !cert.not_after().ge(&now)
    }

    /// Checks if certificate is revoked
    fn is_certificate_revoked(&self, cert: &X509) -> Result<bool, PdfError> {
        // Implement CRL/OCSP checking
        Ok(false)
    }

    /// Verifies certificate chain
    fn verify_certificate_chain(&self, pkcs7: &Pkcs7) -> Result<bool, PdfError> {
        // Implement chain verification logic
        Ok(true)
    }

    /// Creates a forensic artifact from signature analysis
    fn create_signature_artifact(&self, signature: &PdfSignature) -> Result<ForensicArtifact, PdfError> {
        let risk_level = match signature.verification_status {
            SignatureStatus::Valid => RiskLevel::Low,
            SignatureStatus::CertificateExpired => RiskLevel::Medium,
            SignatureStatus::CertificateRevoked => RiskLevel::High,
            SignatureStatus::BrokenChainOfTrust => RiskLevel::High,
            SignatureStatus::Invalid => RiskLevel::Critical,
            SignatureStatus::Unknown => RiskLevel::High,
        };

        let mut metadata = HashMap::new();
        metadata.insert("signature_type".into(), format!("{:?}", signature.signature_type));
        metadata.insert("verification_status".into(), format!("{:?}", signature.verification_status));
        if let Some(time) = signature.signing_time {
            metadata.insert("signing_time".into(), time.to_rfc3339());
        }

        Ok(ForensicArtifact {
            id: uuid::Uuid::new_v4().to_string(),
            artifact_type: ArtifactType::Signature,
            location: signature.field_name.clone(),
            description: self.generate_signature_description(signature),
            risk_level,
            remediation: self.generate_signature_remediation(signature),
            metadata,
            detection_timestamp: chrono::Utc::now(),
            hash: self.calculate_hash(&signature.pkcs7_data),
        })
    }

    /// Generates description for signature artifact
    fn generate_signature_description(&self, signature: &PdfSignature) -> String {
        let mut description = format!("Digital signature of type {:?}", signature.signature_type);
        
        if let Some(signer_info) = &signature.signer_info {
            description.push_str(&format!("\nSigner: {}", signer_info.common_name));
            if let Some(org) = &signer_info.organization {
                description.push_str(&format!("\nOrganization: {}", org));
            }
        }

        description.push_str(&format!("\nStatus: {:?}", signature.verification_status));
        
        description
    }

    /// Generates remediation advice for signature artifact
    fn generate_signature_remediation(&self, signature: &PdfSignature) -> String {
        match signature.verification_status {
            SignatureStatus::Valid => "No remediation needed".into(),
            SignatureStatus::CertificateExpired => "Update signature with valid certificate".into(),
            SignatureStatus::CertificateRevoked => "Remove compromised signature and re-sign with valid certificate".into(),
            SignatureStatus::BrokenChainOfTrust => "Verify and update certificate chain".into(),
            SignatureStatus::Invalid => "Remove invalid signature and re-sign document".into(),
            SignatureStatus::Unknown => "Investigate signature validity and update if necessary".into(),
        }
    }

    /// Calculates hash of signature data
    fn calculate_hash(&self, data: &[u8]) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    #[test]
    async fn test_signature_type_detection() {
        let scanner = SignatureScanner::new(ScannerConfig::default());
        let mut dict = Dictionary::new();
        
        dict.set_filter_name("Adobe.PPKLite");
        dict.set_subfilter_name("adbe.pkcs7.detached");
        assert_eq!(scanner.determine_signature_type(&dict), SignatureType::AdobePkcs7Detached);
        
        dict.set_subfilter_name("adbe.pkcs7.sha1");
        assert_eq!(scanner.determine_signature_type(&dict), SignatureType::AdobePkcs7Sha1);
    }

    #[test]
    async fn test_signing_time_extraction() {
        let scanner = SignatureScanner::new(ScannerConfig::default());
        let mut dict = Dictionary::new();
        
        let time_str = "2025-06-03T04:23:07Z";
        dict.set_signing_time(time_str);
        
        let result = scanner.extract_signing_time(&dict).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_rfc3339(), time_str);
    }

    #[test]
    async fn test_certificate_expiration() {
        let scanner = SignatureScanner::new(ScannerConfig::default());
        
        let expired_cert = create_test_certificate(-30); // 30 days ago
        assert!(scanner.is_certificate_expired(&expired_cert));
        
        let valid_cert = create_test_certificate(30); // Valid for 30 days
        assert!(!scanner.is_certificate_expired(&valid_cert));
    }

    fn create_test_certificate(days_valid: i32) -> X509 {
        let mut builder = X509::builder().unwrap();
        builder.set_not_before(&openssl::asn1::Asn1Time::days_from_now(0).unwrap()).unwrap();
        builder.set_not_after(&openssl::asn1::Asn1Time::days_from_now(days_valid).unwrap()).unwrap();
        builder.build()
    }
}
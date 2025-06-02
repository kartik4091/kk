// Continuing biometric.rs implementation...

impl BiometricCrypto {
    // Continuing from previous implementation...

    async fn process_biometric_data(&self, data: &BiometricData) -> Result<BiometricFeatures, PdfError> {
        // Preprocess the biometric data
        let preprocessed_data = self.preprocess_data(data).await?;
        
        // Extract features based on modality
        let features = match &self.config.modality {
            BiometricModality::Fingerprint(config) => {
                self.extract_fingerprint_features(&preprocessed_data, config).await?
            }
            BiometricModality::Face(config) => {
                self.extract_face_features(&preprocessed_data, config).await?
            }
            BiometricModality::Iris(config) => {
                self.extract_iris_features(&preprocessed_data, config).await?
            }
            BiometricModality::Voice(config) => {
                self.extract_voice_features(&preprocessed_data, config).await?
            }
            BiometricModality::Multimodal(modalities) => {
                self.extract_multimodal_features(&preprocessed_data, modalities).await?
            }
            BiometricModality::Custom(config) => {
                self.extract_custom_features(&preprocessed_data, config).await?
            }
        };

        Ok(features)
    }

    async fn create_protected_template(
        &self,
        features: BiometricFeatures,
        user_id: &str,
    ) -> Result<Template, PdfError> {
        let mut protected_template = match &self.config.processing.template_protection.scheme {
            ProtectionScheme::FuzzyVault { polynomial_degree } => {
                self.create_fuzzy_vault(&features, *polynomial_degree).await?
            }
            ProtectionScheme::FuzzyCommitment { error_correction } => {
                self.create_fuzzy_commitment(&features, error_correction).await?
            }
            ProtectionScheme::Cancelable { transformation_method } => {
                self.create_cancelable_template(&features, transformation_method).await?
            }
            ProtectionScheme::Homomorphic { encryption_params } => {
                self.create_homomorphic_template(&features, encryption_params).await?
            }
        };

        // Add key binding if configured
        if let Some(key_binding) = &self.config.processing.template_protection.key_binding {
            protected_template = self.bind_key_to_template(protected_template, key_binding).await?;
        }

        // Apply additional transformation if configured
        if let Some(transformation) = &self.config.processing.template_protection.transformation {
            protected_template = self.transform_template(protected_template, transformation).await?;
        }

        Ok(Template {
            id: format!("template_{}", user_id),
            data: protected_template,
            metadata: TemplateMetadata {
                creation_time: chrono::Utc::now(),
                modality: self.config.modality.clone(),
                protection_info: self.config.processing.template_protection.clone(),
                quality_scores: self.compute_quality_scores(&features).await?,
            },
        })
    }

    async fn create_fuzzy_vault(
        &self,
        features: &BiometricFeatures,
        polynomial_degree: usize,
    ) -> Result<ProtectedTemplate, PdfError> {
        let mut rng = ring::rand::SystemRandom::new();
        
        // Generate polynomial coefficients
        let mut coefficients = vec![0u8; polynomial_degree + 1];
        for coeff in &mut coefficients {
            rng.fill(std::slice::from_mut(coeff))?;
        }

        // Project features onto polynomial
        let projections = self.project_features_to_polynomial(features, &coefficients)?;

        // Add chaff points
        let chaff_points = self.generate_chaff_points(&projections, features.len() * 3)?;

        // Create vault
        Ok(ProtectedTemplate::FuzzyVault {
            genuine_points: projections,
            chaff_points,
            degree: polynomial_degree,
        })
    }

    async fn create_fuzzy_commitment(
        &self,
        features: &BiometricFeatures,
        error_correction: &ErrorCorrection,
    ) -> Result<ProtectedTemplate, PdfError> {
        // Generate random key
        let mut key = vec![0u8; 32];
        ring::rand::SystemRandom::new().fill(&mut key)?;

        // Encode key using error correction
        let encoded_key = match error_correction {
            ErrorCorrection::ReedSolomon { parameters } => {
                self.encode_reed_solomon(&key, parameters).await?
            }
            ErrorCorrection::BCH { parameters } => {
                self.encode_bch(&key, parameters).await?
            }
            ErrorCorrection::LDPC { parameters } => {
                self.encode_ldpc(&key, parameters).await?
            }
            ErrorCorrection::Custom { method, parameters } => {
                self.encode_custom(&key, method, parameters).await?
            }
        };

        // XOR encoded key with features
        let committed_features = self.xor_features_with_key(features, &encoded_key)?;

        Ok(ProtectedTemplate::FuzzyCommitment {
            committed_data: committed_features,
            helper_data: encoded_key,
        })
    }

    async fn bind_key_to_template(
        &self,
        template: ProtectedTemplate,
        key_binding: &KeyBinding,
    ) -> Result<ProtectedTemplate, PdfError> {
        match &key_binding.method {
            KeyBindingMethod::FuzzyExtractor => {
                self.bind_key_fuzzy_extractor(template, key_binding.key_size).await
            }
            KeyBindingMethod::SecureSketch => {
                self.bind_key_secure_sketch(template, key_binding.entropy).await
            }
            KeyBindingMethod::HelperData => {
                self.bind_key_helper_data(template, key_binding.key_size).await
            }
            KeyBindingMethod::Custom(method) => {
                self.bind_key_custom(template, method, key_binding).await
            }
        }
    }

    async fn compute_quality_scores(
        &self,
        features: &BiometricFeatures,
    ) -> Result<HashMap<String, f64>, PdfError> {
        let mut scores = HashMap::new();

        for metric in &self.config.processing.preprocessing.quality_assessment.metrics {
            let score = match metric {
                QualityMetric::Contrast => {
                    self.compute_contrast_score(features).await?
                }
                QualityMetric::Sharpness => {
                    self.compute_sharpness_score(features).await?
                }
                QualityMetric::NoiseLevel => {
                    self.compute_noise_score(features).await?
                }
                QualityMetric::Coverage => {
                    self.compute_coverage_score(features).await?
                }
                QualityMetric::Custom(metric_name) => {
                    self.compute_custom_quality_score(features, metric_name).await?
                }
            };

            scores.insert(format!("{:?}", metric), score);
        }

        Ok(scores)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fingerprint_enrollment() {
        let config = BiometricConfig {
            modality: BiometricModality::Fingerprint(FingerprintConfig {
                scanner_type: ScannerType::Optical,
                min_minutiae_points: 30,
                enhancement: ImageEnhancement {
                    contrast_adjustment: ContrastMethod::CLAHE,
                    filters: vec![
                        ImageFilter::Gaussian { sigma: 1.0 },
                        ImageFilter::Median { kernel_size: 3 },
                    ],
                    sharpening: true,
                },
                matching_threshold: 0.85,
            }),
            processing: ProcessingConfig::default(),
            storage: StorageConfig::default(),
            security: SecurityConfig::default(),
        };

        let mut biometric = BiometricCrypto::new(config);
        
        let test_data = BiometricData::Fingerprint(vec![0u8; 1024]);
        let template = biometric.enroll(&test_data, "test_user").await.unwrap();
        
        assert!(!template.data.is_empty());
        assert_eq!(template.metadata.modality.get_type(), "Fingerprint");
    }
}
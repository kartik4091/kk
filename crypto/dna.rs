// Continuing dna.rs implementation...

impl DNACrypto {
    // Continuing from previous implementation...

    async fn encode_binary(&self, data: &[u8], config: &BinaryEncoding) -> Result<DNASequence, PdfError> {
        let mut sequence = Vec::new();
        let mut gc_count = 0;
        let mut last_base = None;
        let mut homopolymer_count = 0;

        for byte in data {
            for i in (0..8).rev() {
                let bit = (byte >> i) & 1;
                
                // Get potential bases for this bit based on mapping
                let potential_bases = self.get_bases_for_bit(bit, &config.mapping)?;
                
                // Choose base considering GC content and homopolymer constraints
                let chosen_base = self.choose_optimal_base(
                    &potential_bases,
                    last_base,
                    homopolymer_count,
                    gc_count,
                    sequence.len(),
                    &config.gc_content,
                    config.homopolymer_limit,
                )?;

                // Update counters
                if chosen_base == 'G' || chosen_base == 'C' {
                    gc_count += 1;
                }
                
                if last_base == Some(chosen_base) {
                    homopolymer_count += 1;
                } else {
                    homopolymer_count = 1;
                }

                sequence.push(chosen_base);
                last_base = Some(chosen_base);
            }
        }

        Ok(DNASequence {
            sequence,
            metadata: SequenceMetadata {
                encoding_type: "Binary".to_string(),
                gc_content: gc_count as f64 / sequence.len() as f64,
                length: sequence.len(),
                error_correction: self.config.error_correction.method.clone(),
            },
        })
    }

    async fn encode_quaternary(
        &self,
        data: &[u8],
        config: &QuaternaryEncoding,
    ) -> Result<DNASequence, PdfError> {
        let mut sequence = Vec::new();
        let base_pairs = ['A', 'T', 'C', 'G'];

        for chunk in data.chunks(2) {
            let value = if chunk.len() == 2 {
                ((chunk[0] as u16) << 8) | (chunk[1] as u16)
            } else {
                (chunk[0] as u16) << 8
            };

            // Convert to base-4 representation
            let mut quaternary = Vec::new();
            let mut temp = value;
            
            for _ in 0..8 {  // Each 16-bit value yields 8 DNA bases
                let base_index = (temp & 0b11) as usize;
                quaternary.push(base_pairs[base_index]);
                temp >>= 2;
            }

            // Apply sequence rules
            self.apply_sequence_rules(&mut quaternary, &config.sequence_rules)?;
            
            // Optimize based on parameters
            self.optimize_quaternary_sequence(
                &mut quaternary,
                &config.optimization_params,
            )?;

            sequence.extend(quaternary);
        }

        Ok(DNASequence {
            sequence,
            metadata: SequenceMetadata {
                encoding_type: "Quaternary".to_string(),
                gc_content: self.calculate_gc_content(&sequence),
                length: sequence.len(),
                error_correction: self.config.error_correction.method.clone(),
            },
        })
    }

    async fn encode_huffman(
        &self,
        data: &[u8],
        config: &HuffmanEncoding,
    ) -> Result<DNASequence, PdfError> {
        // Build or retrieve Huffman tree
        let tree = match &config.tree_structure {
            Some(tree) => tree.clone(),
            None => self.build_huffman_tree(&config.frequency_table)?,
        };

        let mut sequence = Vec::new();
        let mut bit_buffer = BitBuffer::new();

        // Encode each byte using Huffman tree
        for &byte in data {
            let encoding = self.get_huffman_encoding(byte, &tree)?;
            bit_buffer.extend(&encoding);

            // Convert complete DNA codons
            while bit_buffer.len() >= 2 {
                let codon = self.bits_to_dna(bit_buffer.take(2))?;
                sequence.push(codon);
            }
        }

        // Handle remaining bits
        if !bit_buffer.is_empty() {
            let padding = 2 - bit_buffer.len();
            bit_buffer.pad(padding);
            let codon = self.bits_to_dna(bit_buffer.take(2))?;
            sequence.push(codon);
        }

        Ok(DNASequence {
            sequence,
            metadata: SequenceMetadata {
                encoding_type: "Huffman".to_string(),
                gc_content: self.calculate_gc_content(&sequence),
                length: sequence.len(),
                error_correction: self.config.error_correction.method.clone(),
            },
        })
    }

    async fn apply_error_correction(&self, sequence: &DNASequence) -> Result<DNASequence, PdfError> {
        match &self.config.error_correction.method {
            ErrorCorrectionMethod::ReedSolomon(params) => {
                self.apply_reed_solomon(sequence, params).await
            }
            ErrorCorrectionMethod::LDPC(params) => {
                self.apply_ldpc(sequence, params).await
            }
            ErrorCorrectionMethod::Repetition(params) => {
                self.apply_repetition(sequence, params).await
            }
            ErrorCorrectionMethod::Custom(params) => {
                self.apply_custom_error_correction(sequence, params).await
            }
        }
    }

    async fn verify_and_correct(&self, sequence: &DNASequence) -> Result<DNASequence, PdfError> {
        // First, verify the sequence
        let errors = self.detect_errors(sequence).await?;
        
        if errors.is_empty() {
            return Ok(sequence.clone());
        }

        // Apply repair strategy
        match &self.config.error_correction.repair_strategy.method {
            RepairMethod::Substitution => {
                self.repair_by_substitution(sequence, &errors).await
            }
            RepairMethod::Insertion => {
                self.repair_by_insertion(sequence, &errors).await
            }
            RepairMethod::Deletion => {
                self.repair_by_deletion(sequence, &errors).await
            }
            RepairMethod::Hybrid(methods) => {
                self.repair_hybrid(sequence, &errors, methods).await
            }
        }
    }

    async fn optimize_sequence(&self, sequence: &DNASequence) -> Result<DNASequence, PdfError> {
        let mut optimized = sequence.clone();

        if self.config.encoding.optimization.gc_balancing {
            optimized = self.balance_gc_content(&optimized).await?;
        }

        if self.config.encoding.optimization.secondary_structure.prevent_hairpins {
            optimized = self.prevent_hairpins(&optimized).await?;
        }

        // Verify constraints
        self.verify_constraints(&optimized, &self.config.encoding.constraints)?;

        Ok(optimized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_binary_encoding() {
        let config = DNAConfig {
            encoding: EncodingConfig {
                scheme: EncodingScheme::Binary(BinaryEncoding {
                    mapping: HashMap::from([
                        ("0".to_string(), "AT".to_string()),
                        ("1".to_string(), "GC".to_string()),
                    ]),
                    gc_content: GCContent {
                        min_ratio: 0.4,
                        max_ratio: 0.6,
                        target_ratio: 0.5,
                    },
                    homopolymer_limit: 3,
                }),
                optimization: OptimizationConfig::default(),
                constraints: EncodingConstraints::default(),
            },
            storage: StorageConfig::default(),
            error_correction: ErrorCorrectionConfig::default(),
            security: SecurityConfig::default(),
        };

        let dna_crypto = DNACrypto::new(config);
        let test_data = vec![0x55, 0xAA]; // 01010101 10101010
        
        let encoded = dna_crypto.encode(&test_data).await.unwrap();
        let decoded = dna_crypto.decode(&encoded).await.unwrap();
        
        assert_eq!(decoded, test_data);
    }
}
// Continuing blockchain.rs implementation...

impl BlockchainCrypto {
    // Continuing from previous implementation...

    pub async fn create_block(&mut self, transactions: Vec<Transaction>) -> Result<Block, PdfError> {
        let mut consensus = self.consensus.write().await;
        let state = self.state.read().await;
        let mempool = self.mempool.read().await;

        // Validate transactions
        let valid_transactions = self.validate_transactions(&transactions).await?;

        // Create block according to consensus mechanism
        let block = match &self.config.consensus.mechanism {
            ConsensusMechanism::PoW(config) => {
                self.create_pow_block(valid_transactions, &state, config).await?
            }
            ConsensusMechanism::PoS(config) => {
                self.create_pos_block(valid_transactions, &state, config).await?
            }
            ConsensusMechanism::DPoS(config) => {
                self.create_dpos_block(valid_transactions, &state, config).await?
            }
            ConsensusMechanism::PBFT(config) => {
                self.create_pbft_block(valid_transactions, &state, config).await?
            }
            ConsensusMechanism::Hotstuff(config) => {
                self.create_hotstuff_block(valid_transactions, &state, config).await?
            }
            ConsensusMechanism::Custom(config) => {
                self.create_custom_block(valid_transactions, &state, config).await?
            }
        };

        // Validate block
        consensus.validate_block(&block).await?;

        Ok(block)
    }

    pub async fn add_transaction(&mut self, transaction: Transaction) -> Result<TransactionId, PdfError> {
        let mut mempool = self.mempool.write().await;
        
        // Validate transaction
        self.validate_transaction(&transaction).await?;

        // Add to mempool
        let tx_id = mempool.add_transaction(transaction).await?;

        Ok(tx_id)
    }

    pub async fn validate_chain(&self) -> Result<bool, PdfError> {
        let state = self.state.read().await;
        let consensus = self.consensus.read().await;

        for block in state.get_blocks().await? {
            // Validate block hash
            if !self.validate_block_hash(&block).await? {
                return Ok(false);
            }

            // Validate block consensus
            if !consensus.validate_block(&block).await? {
                return Ok(false);
            }

            // Validate block transactions
            for transaction in block.transactions {
                if !self.validate_transaction(&transaction).await? {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    pub async fn get_balance(&self, address: &Address) -> Result<Balance, PdfError> {
        let state = self.state.read().await;
        state.get_balance(address).await
    }

    pub async fn get_transaction(&self, tx_id: &TransactionId) -> Result<Option<Transaction>, PdfError> {
        let state = self.state.read().await;
        state.get_transaction(tx_id).await
    }

    pub async fn get_block(&self, block_hash: &BlockHash) -> Result<Option<Block>, PdfError> {
        let state = self.state.read().await;
        state.get_block(block_hash).await
    }

    pub async fn get_block_by_height(&self, height: u64) -> Result<Option<Block>, PdfError> {
        let state = self.state.read().await;
        state.get_block_by_height(height).await
    }

    // Private helper methods

    async fn validate_transactions(&self, transactions: &[Transaction]) -> Result<Vec<Transaction>, PdfError> {
        let mut valid_transactions = Vec::new();
        
        for transaction in transactions {
            if self.validate_transaction(transaction).await? {
                valid_transactions.push(transaction.clone());
            }
        }

        Ok(valid_transactions)
    }

    async fn validate_transaction(&self, transaction: &Transaction) -> Result<bool, PdfError> {
        // Validate signature
        if !self.validate_signature(transaction).await? {
            return Ok(false);
        }

        // Validate balance
        if !self.validate_balance(transaction).await? {
            return Ok(false);
        }

        // Validate nonce
        if !self.validate_nonce(transaction).await? {
            return Ok(false);
        }

        Ok(true)
    }

    async fn validate_block_hash(&self, block: &Block) -> Result<bool, PdfError> {
        let calculated_hash = self.calculate_block_hash(block).await?;
        Ok(calculated_hash == block.hash)
    }

    async fn calculate_block_hash(&self, block: &Block) -> Result<BlockHash, PdfError> {
        let mut hasher = Context::new(&SHA256);
        
        // Add previous hash
        hasher.update(&block.previous_hash);
        
        // Add timestamp
        hasher.update(&block.timestamp.to_be_bytes());
        
        // Add transactions
        for transaction in &block.transactions {
            hasher.update(&transaction.hash());
        }
        
        // Add nonce
        hasher.update(&block.nonce.to_be_bytes());

        Ok(hasher.finish().as_ref().to_vec())
    }

    async fn create_pow_block(
        &self,
        transactions: Vec<Transaction>,
        state: &BlockchainState,
        config: &PoWConfig,
    ) -> Result<Block, PdfError> {
        let mut block = Block {
            version: 1,
            previous_hash: state.get_latest_block_hash().await?,
            timestamp: chrono::Utc::now().timestamp(),
            transactions,
            nonce: 0,
            hash: Vec::new(),
            height: state.get_height().await? + 1,
            difficulty: self.calculate_difficulty(config).await?,
        };

        // Mine block
        while !self.check_pow_solution(&block, config).await? {
            block.nonce += 1;
            block.hash = self.calculate_block_hash(&block).await?;
        }

        Ok(block)
    }

    async fn check_pow_solution(&self, block: &Block, config: &PoWConfig) -> Result<bool, PdfError> {
        let hash = self.calculate_block_hash(block).await?;
        let difficulty_target = self.calculate_difficulty_target(block.difficulty);
        
        Ok(hash.as_slice() <= difficulty_target.as_slice())
    }

    async fn calculate_difficulty(&self, config: &PoWConfig) -> Result<u64, PdfError> {
        let state = self.state.read().await;
        
        match &config.difficulty_adjustment {
            DifficultyAdjustment {
                algorithm: DifficultyAlgorithm::Bitcoin,
                target_time_span,
                adjustment_factor,
            } => {
                let last_adjustment_block = state.get_last_difficulty_adjustment_block().await?;
                let latest_block = state.get_latest_block().await?;
                
                let actual_time_span = latest_block.timestamp - last_adjustment_block.timestamp;
                let target_time_span = target_time_span.as_secs() as i64;
                
                let mut new_difficulty = latest_block.difficulty as f64;
                new_difficulty *= (actual_time_span as f64) / (target_time_span as f64);
                new_difficulty *= adjustment_factor;
                
                Ok(new_difficulty as u64)
            }
            // Add other difficulty adjustment algorithms...
            _ => Ok(latest_block.difficulty),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pow_mining() {
        let config = BlockchainConfig {
            consensus: ConsensusConfig {
                mechanism: ConsensusMechanism::PoW(PoWConfig {
                    difficulty_adjustment: DifficultyAdjustment {
                        algorithm: DifficultyAlgorithm::Bitcoin,
                        target_time_span: std::time::Duration::from_secs(14 * 24 * 60 * 60),
                        adjustment_factor: 4.0,
                    },
                    hash_algorithm: HashAlgorithm::SHA256,
                    target_block_time: std::time::Duration::from_secs(600),
                    max_block_size: 1_000_000,
                }),
                block_time: std::time::Duration::from_secs(600),
                validators: ValidatorConfig::default(),
                finality: FinalityConfig::default(),
            },
            cryptography: CryptoConfig::default(),
            networking: NetworkConfig::default(),
            storage: StorageConfig::default(),
        };

        let mut blockchain = BlockchainCrypto::new(config);
        blockchain.initialize_blockchain().await.unwrap();

        let transactions = vec![
            Transaction::new("sender", "receiver", 100),
            Transaction::new("sender2", "receiver2", 50),
        ];

        let block = blockchain.create_block(transactions).await.unwrap();
        
        assert!(blockchain.validate_block_hash(&block).await.unwrap());
        assert!(blockchain.validate_chain().await.unwrap());
    }
}
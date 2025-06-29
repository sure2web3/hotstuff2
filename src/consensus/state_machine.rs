use std::collections::{HashMap, VecDeque};
use crate::types::{Block, Hash, QuorumCert, Transaction};
use crate::error::HotStuffError;

/// The state machine that executes committed transactions
#[async_trait::async_trait]
pub trait StateMachine: Send + Sync {
    /// Execute a block of transactions and return the new state hash
    fn execute_block(&mut self, block: &Block) -> Result<Hash, HotStuffError>;
    
    /// Apply a single transaction asynchronously
    async fn apply_transaction(&mut self, transaction: Transaction) -> Result<(), HotStuffError>;
    
    /// Get the current state hash
    fn state_hash(&self) -> Hash;
    
    /// Get the current height
    fn height(&self) -> u64;
    
    /// Reset to a specific state (for recovery)
    fn reset_to_state(&mut self, state_hash: Hash, height: u64) -> Result<(), HotStuffError>;
}

/// A simple key-value state machine for demonstration
#[derive(Debug, Clone)]
pub struct KVStateMachine {
    state: HashMap<String, String>,
    height: u64,
    state_hash: Hash,
}

/// State snapshot for backup and recovery
#[derive(Debug, Clone)]
pub struct StateSnapshot {
    pub state: HashMap<String, String>,
    pub height: u64,
    pub state_hash: Hash,
}

impl KVStateMachine {
    pub fn new() -> Self {
        Self {
            state: HashMap::new(),
            height: 0,
            state_hash: Hash::zero(),
        }
    }

    fn compute_state_hash(&self) -> Hash {
        // Simple state hash computation
        let mut data = Vec::new();
        let mut sorted_keys: Vec<_> = self.state.keys().collect();
        sorted_keys.sort();
        
        for key in sorted_keys {
            data.extend_from_slice(key.as_bytes());
            if let Some(value) = self.state.get(key) {
                data.extend_from_slice(value.as_bytes());
            }
        }
        data.extend_from_slice(&self.height.to_be_bytes());
        
        Hash::from_bytes(&data)
    }
}

#[async_trait::async_trait]
impl StateMachine for KVStateMachine {
    fn execute_block(&mut self, block: &Block) -> Result<Hash, HotStuffError> {
        // Parse transactions and apply them
        for tx in &block.transactions {
            self.execute_transaction(tx)?;
        }
        
        self.height = block.height;
        self.state_hash = self.compute_state_hash();
        Ok(self.state_hash)
    }
    
    async fn apply_transaction(&mut self, transaction: Transaction) -> Result<(), HotStuffError> {
        self.execute_transaction(&transaction)
    }

    fn state_hash(&self) -> Hash {
        self.state_hash
    }

    fn height(&self) -> u64 {
        self.height
    }

    fn reset_to_state(&mut self, state_hash: Hash, height: u64) -> Result<(), HotStuffError> {
        // In a real implementation, this would restore from a checkpoint
        self.state_hash = state_hash;
        self.height = height;
        Ok(())
    }
}

impl KVStateMachine {
    /// Execute a single transaction
    fn execute_transaction(&mut self, tx: &Transaction) -> Result<(), HotStuffError> {
        // Parse transaction data - expecting "SET key value" or "DEL key"
        let tx_str = String::from_utf8_lossy(&tx.data);
        let parts: Vec<&str> = tx_str.split_whitespace().collect();
        
        match parts.get(0) {
            Some(&"SET") if parts.len() == 3 => {
                let key = parts[1].to_string();
                let value = parts[2].to_string();
                self.state.insert(key, value);
                Ok(())
            }
            Some(&"DEL") if parts.len() == 2 => {
                let key = parts[1].to_string();
                self.state.remove(&key);
                Ok(())
            }
            Some(&"INC") if parts.len() == 2 => {
                let key = parts[1].to_string();
                let current: i64 = self.state.get(&key)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                self.state.insert(key, (current + 1).to_string());
                Ok(())
            }
            _ => {
                // For unrecognized transactions, just store the raw data using transaction id
                self.state.insert(tx.id.clone(), String::from_utf8_lossy(&tx.data).to_string());
                Ok(())
            }
        }
    }

    /// Get a value from the state
    pub fn get(&self, key: &str) -> Option<&String> {
        self.state.get(key)
    }

    /// Get all keys in the state
    pub fn keys(&self) -> Vec<String> {
        self.state.keys().cloned().collect()
    }

    /// Get the size of the state
    pub fn size(&self) -> usize {
        self.state.len()
    }

    /// Clear all state
    pub fn clear(&mut self) {
        self.state.clear();
        self.height = 0;
        self.state_hash = Hash::zero();
    }

    /// Create a snapshot of the current state
    pub fn create_snapshot(&self) -> StateSnapshot {
        StateSnapshot {
            state: self.state.clone(),
            height: self.height,
            state_hash: self.state_hash,
        }
    }

    /// Restore from a snapshot
    pub fn restore_from_snapshot(&mut self, snapshot: StateSnapshot) {
        self.state = snapshot.state;
        self.height = snapshot.height;
        self.state_hash = snapshot.state_hash;
    }
}

/// Chain state for HotStuff-2 consensus
#[derive(Debug, Clone)]
pub struct ChainView {
    pub committed_blocks: VecDeque<Block>,
    pub pending_blocks: HashMap<Hash, Block>,
    pub qc_chain: VecDeque<QuorumCert>, // Chain of QCs for the three-chain rule
    pub max_committed_height: u64,
    pub max_pending_height: u64,
}

impl ChainView {
    pub fn new() -> Self {
        Self {
            committed_blocks: VecDeque::new(),
            pending_blocks: HashMap::new(),
            qc_chain: VecDeque::new(),
            max_committed_height: 0,
            max_pending_height: 0,
        }
    }

    /// Add a block to the pending blocks
    pub fn add_pending_block(&mut self, block: Block) {
        self.max_pending_height = self.max_pending_height.max(block.height);
        self.pending_blocks.insert(block.hash(), block);
    }

    /// Add a QC to the QC chain
    pub fn add_qc(&mut self, qc: QuorumCert) {
        self.qc_chain.push_back(qc);
        
        // Keep only the last few QCs for the three-chain rule
        while self.qc_chain.len() > 10 {
            self.qc_chain.pop_front();
        }
    }

    /// Check if we can commit according to the three-chain rule
    pub fn check_three_chain_commit(&self) -> Option<Hash> {
        if self.qc_chain.len() < 3 {
            return None;
        }

        // Get the last three QCs
        let len = self.qc_chain.len();
        let qc1 = &self.qc_chain[len - 3];
        let qc2 = &self.qc_chain[len - 2];
        let qc3 = &self.qc_chain[len - 1];

        // Check if they form a valid three-chain
        if qc2.height == qc1.height + 1 && qc3.height == qc2.height + 1 {
            // We can commit the block from qc1
            Some(qc1.block_hash)
        } else {
            None
        }
    }

    /// Commit a block and move it from pending to committed
    pub fn commit_block(&mut self, block_hash: Hash) -> Option<Block> {
        if let Some(block) = self.pending_blocks.remove(&block_hash) {
            self.max_committed_height = self.max_committed_height.max(block.height);
            self.committed_blocks.push_back(block.clone());
            
            // Keep only recent committed blocks
            while self.committed_blocks.len() > 100 {
                self.committed_blocks.pop_front();
            }
            
            Some(block)
        } else {
            None
        }
    }

    /// Get a block by hash (from either committed or pending)
    pub fn get_block(&self, hash: &Hash) -> Option<&Block> {
        // First check pending blocks
        if let Some(block) = self.pending_blocks.get(hash) {
            return Some(block);
        }
        
        // Then check committed blocks
        self.committed_blocks.iter().find(|b| &b.hash() == hash)
    }

    /// Check if a block extends the given QC
    pub fn extends_qc(&self, block: &Block, qc: &QuorumCert) -> bool {
        // Simple check: block height should be greater than QC height
        // In a full implementation, we'd verify the actual chain connection
        block.height > qc.height
    }
}

/// Safety rules for HotStuff-2
pub struct SafetyRules {
    locked_qc: Option<QuorumCert>,
    preferred_round: u64,
}

impl SafetyRules {
    pub fn new() -> Self {
        Self {
            locked_qc: None,
            preferred_round: 0,
        }
    }

    /// Check if it's safe to vote for a block
    pub fn safe_to_vote(&self, block: &Block, high_qc: Option<&QuorumCert>) -> bool {
        // HotStuff-2 safety rule: can vote if block extends locked QC or we have higher QC
        if let Some(locked_qc) = &self.locked_qc {
            if let Some(high_qc) = high_qc {
                // Can vote if our high QC is higher than locked QC
                if high_qc.height > locked_qc.height {
                    return true;
                }
            }
            // Can vote if block extends our locked QC
            block.height > locked_qc.height
        } else {
            // No locked QC, safe to vote for any valid block
            true
        }
    }

    /// Update the locked QC (typically done in pre-commit phase)
    pub fn update_locked_qc(&mut self, qc: QuorumCert) {
        if self.locked_qc.as_ref().map(|q| q.height).unwrap_or(0) < qc.height {
            self.locked_qc = Some(qc);
        }
    }

    /// Update the preferred round
    pub fn update_preferred_round(&mut self, round: u64) {
        self.preferred_round = self.preferred_round.max(round);
    }

    pub fn locked_qc(&self) -> Option<&QuorumCert> {
        self.locked_qc.as_ref()
    }

    pub fn preferred_round(&self) -> u64 {
        self.preferred_round
    }
}

#[cfg(any(test, feature = "byzantine"))]
/// A simple test state machine for unit tests
#[derive(Debug, Clone)]
pub struct TestStateMachine {
    state: HashMap<String, String>,
    height: u64,
    state_hash: Hash,
}

#[cfg(any(test, feature = "byzantine"))]
impl TestStateMachine {
    pub fn new() -> Self {
        Self {
            state: HashMap::new(),
            height: 0,
            state_hash: Hash::zero(),
        }
    }

    fn compute_state_hash(&self) -> Hash {
        // Simple deterministic hash computation for testing
        let mut keys: Vec<_> = self.state.keys().collect();
        keys.sort();
        let state_str = keys.iter()
            .map(|k| format!("{}:{}", k, self.state[*k]))
            .collect::<Vec<_>>()
            .join(",");
        Hash::from_bytes(format!("{}:{}", self.height, state_str).as_bytes())
    }
}

#[cfg(any(test, feature = "byzantine"))]
#[async_trait::async_trait]
impl StateMachine for TestStateMachine {
    fn execute_block(&mut self, block: &Block) -> Result<Hash, HotStuffError> {
        for tx in &block.transactions {
            // Parse transaction data as simple "key=value" format
            let tx_str = String::from_utf8_lossy(&tx.data);
            if let Some((key, value)) = tx_str.split_once('=') {
                self.state.insert(key.trim().to_string(), value.trim().to_string());
            } else {
                // Fall back to using transaction ID as key
                self.state.insert(tx.id.clone(), tx_str.to_string());
            }
        }
        self.height = block.height;
        self.state_hash = self.compute_state_hash();
        Ok(self.state_hash)
    }

    async fn apply_transaction(&mut self, transaction: Transaction) -> Result<(), HotStuffError> {
        // Parse transaction data as simple "key=value" format
        let tx_str = String::from_utf8_lossy(&transaction.data);
        if let Some((key, value)) = tx_str.split_once('=') {
            self.state.insert(key.trim().to_string(), value.trim().to_string());
        } else {
            // Fall back to using transaction ID as key
            self.state.insert(transaction.id.clone(), tx_str.to_string());
        }
        Ok(())
    }

    fn state_hash(&self) -> Hash {
        self.state_hash
    }

    fn height(&self) -> u64 {
        self.height
    }

    fn reset_to_state(&mut self, state_hash: Hash, height: u64) -> Result<(), HotStuffError> {
        self.state.clear();
        self.height = height;
        self.state_hash = state_hash;
        Ok(())
    }
}

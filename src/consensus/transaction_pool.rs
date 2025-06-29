// Production-ready transaction pool and batching system for HotStuff-2
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{debug, info, warn};
use parking_lot::RwLock;
use tokio::sync::{Mutex, Notify};
use tokio::time::{interval, timeout};

use crate::error::HotStuffError;
use crate::types::{Hash, Transaction};

/// Transaction pool with intelligent batching and prioritization
#[derive(Clone)]
pub struct ProductionTxPool {
    // Main transaction storage
    pending_txs: Arc<RwLock<VecDeque<PendingTransaction>>>,
    
    // Transaction index for quick lookups
    tx_index: Arc<RwLock<HashMap<Hash, usize>>>,
    
    // Batching configuration
    config: TxPoolConfig,
    
    // Metrics and monitoring
    stats: Arc<Mutex<TxPoolStats>>,
    
    // Notification system for batch readiness
    batch_ready_notify: Arc<Notify>,
    
    // Fee-based prioritization
    fee_priority_queue: Arc<RwLock<std::collections::BinaryHeap<PrioritizedTx>>>,
}

#[derive(Debug, Clone)]
pub struct TxPoolConfig {
    pub max_pool_size: usize,
    pub max_batch_size: usize,
    pub min_batch_size: usize,
    pub batch_timeout: Duration,
    pub fee_threshold: u64,
    pub enable_fee_prioritization: bool,
    pub max_tx_age: Duration,
    pub eviction_policy: EvictionPolicy,
}

#[derive(Debug, Clone)]
pub enum EvictionPolicy {
    FIFO,
    LowestFee,
    OldestFirst,
    LRU,
}

#[derive(Debug, Clone)]
struct PendingTransaction {
    transaction: Transaction,
    received_at: Instant,
    fee: u64,
    priority_score: u64,
    retry_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PrioritizedTx {
    hash: Hash,
    priority_score: u64,
    fee: u64,
    received_at: Instant,
}

impl Ord for PrioritizedTx {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Higher priority score comes first
        self.priority_score.cmp(&other.priority_score)
            .then_with(|| self.fee.cmp(&other.fee))
            .then_with(|| other.received_at.cmp(&self.received_at)) // Earlier timestamp wins ties
    }
}

impl PartialOrd for PrioritizedTx {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Default)]
pub struct TxPoolStats {
    pub total_received: u64,
    pub total_processed: u64,
    pub total_evicted: u64,
    pub current_pool_size: usize,
    pub average_batch_size: f64,
    pub average_wait_time: Duration,
    pub fee_revenue: u64,
}

impl Default for TxPoolConfig {
    fn default() -> Self {
        Self {
            max_pool_size: 10_000,
            max_batch_size: 1_000,
            min_batch_size: 10,
            batch_timeout: Duration::from_millis(100),
            fee_threshold: 1000, // Minimum fee in smallest units
            enable_fee_prioritization: true,
            max_tx_age: Duration::from_secs(300), // 5 minutes
            eviction_policy: EvictionPolicy::LowestFee,
        }
    }
}

impl ProductionTxPool {
    pub fn new(config: TxPoolConfig) -> Self {
        Self {
            pending_txs: Arc::new(RwLock::new(VecDeque::new())),
            tx_index: Arc::new(RwLock::new(HashMap::new())),
            config,
            stats: Arc::new(Mutex::new(TxPoolStats::default())),
            batch_ready_notify: Arc::new(Notify::new()),
            fee_priority_queue: Arc::new(RwLock::new(std::collections::BinaryHeap::new())),
        }
    }
    
    /// Submit a transaction to the pool with validation
    pub async fn submit_transaction(&self, transaction: Transaction) -> Result<(), HotStuffError> {
        // Basic validation
        self.validate_transaction(&transaction)?;
        
        let tx_hash = transaction.hash();
        
        // Check if transaction already exists
        {
            let index = self.tx_index.read();
            if index.contains_key(&tx_hash) {
                return Err(HotStuffError::InvalidTransaction("Duplicate transaction".to_string()));
            }
        }
        
        // Calculate fee and priority
        let fee = self.extract_fee(&transaction)?;
        let priority_score = self.calculate_priority_score(&transaction, fee);
        
        let pending_tx = PendingTransaction {
            transaction: transaction.clone(),
            received_at: Instant::now(),
            fee,
            priority_score,
            retry_count: 0,
        };
        
        // Check pool capacity and evict if necessary
        self.ensure_capacity().await?;
        
        // Add to pool
        {
            let mut pending_txs = self.pending_txs.write();
            let mut tx_index = self.tx_index.write();
            
            let index = pending_txs.len();
            pending_txs.push_back(pending_tx);
            tx_index.insert(tx_hash, index);
        }
        
        // Add to priority queue if fee prioritization is enabled
        if self.config.enable_fee_prioritization {
            let mut priority_queue = self.fee_priority_queue.write();
            priority_queue.push(PrioritizedTx {
                hash: tx_hash,
                priority_score,
                fee,
                received_at: Instant::now(),
            });
        }
        
        // Update stats
        {
            let mut stats = self.stats.lock().await;
            stats.total_received += 1;
            stats.current_pool_size = self.pending_txs.read().len();
            stats.fee_revenue += fee;
        }
        
        // Notify if batch is ready
        if self.is_batch_ready().await {
            self.batch_ready_notify.notify_waiters();
        }
        
        debug!("Transaction {} submitted to pool (fee: {}, priority: {})", 
               tx_hash, fee, priority_score);
        
        Ok(())
    }
    
    /// Get the next optimal batch of transactions
    pub async fn get_next_batch(&self, max_size: Option<usize>) -> Result<Vec<Transaction>, HotStuffError> {
        let batch_size = max_size.unwrap_or(self.config.max_batch_size);
        
        if self.config.enable_fee_prioritization {
            self.get_priority_batch(batch_size).await
        } else {
            self.get_fifo_batch(batch_size).await
        }
    }
    
    /// Wait for batch to be ready or timeout
    pub async fn wait_for_batch(&self) -> Result<Vec<Transaction>, HotStuffError> {
        // Wait for either batch timeout or sufficient transactions
        let batch_future = async {
            loop {
                if self.is_batch_ready().await {
                    return self.get_next_batch(None).await;
                }
                self.batch_ready_notify.notified().await;
            }
        };
        
        match timeout(self.config.batch_timeout, batch_future).await {
            Ok(result) => result,
            Err(_) => {
                // Timeout reached, return whatever we have if above minimum
                let current_size = self.pending_txs.read().len();
                if current_size >= self.config.min_batch_size {
                    self.get_next_batch(Some(current_size)).await
                } else {
                    Ok(Vec::new())
                }
            }
        }
    }
    
    /// Remove transactions that have been committed
    pub async fn remove_committed_transactions(&self, committed_txs: &[Hash]) -> Result<(), HotStuffError> {
        let mut pending_txs = self.pending_txs.write();
        let mut tx_index = self.tx_index.write();
        let mut priority_queue = self.fee_priority_queue.write();
        
        // Remove from main storage
        let mut removed_count = 0;
        pending_txs.retain(|pending_tx| {
            let should_remove = committed_txs.contains(&pending_tx.transaction.hash());
            if should_remove {
                removed_count += 1;
            }
            !should_remove
        });
        
        // Rebuild index after removal
        tx_index.clear();
        for (i, pending_tx) in pending_txs.iter().enumerate() {
            tx_index.insert(pending_tx.transaction.hash(), i);
        }
        
        // Remove from priority queue (rebuild for simplicity)
        let remaining_txs: std::collections::BinaryHeap<PrioritizedTx> = priority_queue
            .drain()
            .filter(|ptx| !committed_txs.contains(&ptx.hash))
            .collect();
        *priority_queue = remaining_txs;
        
        // Update stats
        {
            let mut stats = self.stats.lock().await;
            stats.total_processed += removed_count as u64;
            stats.current_pool_size = pending_txs.len();
        }
        
        info!("Removed {} committed transactions from pool", removed_count);
        Ok(())
    }
    
    /// Get current pool statistics
    pub async fn get_stats(&self) -> TxPoolStats {
        let mut stats = self.stats.lock().await;
        stats.current_pool_size = self.pending_txs.read().len();
        stats.clone()
    }
    
    /// Get the count of pending transactions
    pub async fn get_pending_count(&self) -> usize {
        self.pending_txs.read().len()
    }
    
    /// Start background maintenance tasks
    pub async fn start_maintenance(&self) -> Result<(), HotStuffError> {
        let pool = self.clone();
        
        let _handle = tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                
                if let Err(e) = pool.cleanup_expired_transactions().await {
                    warn!("Error during transaction cleanup: {}", e);
                }
                
                if let Err(e) = pool.recompute_priorities().await {
                    warn!("Error during priority recomputation: {}", e);
                }
            }
        });
        
        Ok(())
    }
    
    // Private helper methods
    
    fn validate_transaction(&self, transaction: &Transaction) -> Result<(), HotStuffError> {
        // Basic validation
        if transaction.data.is_empty() {
            return Err(HotStuffError::InvalidTransaction("Empty transaction data".to_string()));
        }
        
        if transaction.data.len() > 1024 * 1024 { // 1MB limit
            return Err(HotStuffError::InvalidTransaction("Transaction too large".to_string()));
        }
        
        // Additional validation could include:
        // - Signature verification
        // - Nonce checking
        // - Balance verification
        // - Smart contract validation
        
        Ok(())
    }
    
    fn extract_fee(&self, transaction: &Transaction) -> Result<u64, HotStuffError> {
        // In a real implementation, this would parse the transaction to extract fee
        // For now, use a simple heuristic based on data size
        let base_fee = 1000;
        let size_fee = transaction.data.len() as u64 * 10;
        Ok(base_fee + size_fee)
    }
    
    fn calculate_priority_score(&self, transaction: &Transaction, fee: u64) -> u64 {
        // Priority scoring algorithm
        let fee_score = fee / 1000; // Scale down fee
        let size_penalty = (transaction.data.len() as u64) / 1024; // Penalize large txs
        let time_bonus = 100; // Bonus for being recent
        
        fee_score + time_bonus - size_penalty
    }
    
    async fn ensure_capacity(&self) -> Result<(), HotStuffError> {
        let current_size = self.pending_txs.read().len();
        
        if current_size >= self.config.max_pool_size {
            let to_evict = current_size - self.config.max_pool_size + 1;
            self.evict_transactions(to_evict).await?;
        }
        
        Ok(())
    }
    
    async fn evict_transactions(&self, count: usize) -> Result<(), HotStuffError> {
        let actual_evicted = {
            let mut pending_txs = self.pending_txs.write();
            let mut tx_index = self.tx_index.write();
            let mut evicted_count = 0;
            
            match self.config.eviction_policy {
                EvictionPolicy::FIFO => {
                    for _ in 0..count.min(pending_txs.len()) {
                        if let Some(evicted) = pending_txs.pop_front() {
                            tx_index.remove(&evicted.transaction.hash());
                            evicted_count += 1;
                        }
                    }
                }
                EvictionPolicy::LowestFee => {
                    // Sort by fee and remove lowest
                    let mut txs_with_indices: Vec<_> = pending_txs
                        .iter()
                        .enumerate()
                        .collect();
                    txs_with_indices.sort_by_key(|(_, tx)| tx.fee);
                    
                    let mut to_remove = Vec::new();
                    for (i, _) in txs_with_indices.iter().take(count) {
                        to_remove.push(*i);
                    }
                    
                    // Remove in reverse order to maintain indices
                    to_remove.sort_by(|a, b| b.cmp(a));
                    for i in to_remove {
                        if let Some(evicted) = pending_txs.remove(i) {
                            tx_index.remove(&evicted.transaction.hash());
                            evicted_count += 1;
                        }
                    }
                }
                EvictionPolicy::OldestFirst => {
                    let mut txs_with_indices: Vec<_> = pending_txs
                        .iter()
                        .enumerate()
                        .collect();
                    txs_with_indices.sort_by_key(|(_, tx)| tx.received_at);
                    
                    let mut to_remove = Vec::new();
                    for (i, _) in txs_with_indices.iter().take(count) {
                        to_remove.push(*i);
                    }
                    
                    to_remove.sort_by(|a, b| b.cmp(a));
                    for i in to_remove {
                        if let Some(evicted) = pending_txs.remove(i) {
                            tx_index.remove(&evicted.transaction.hash());
                            evicted_count += 1;
                        }
                    }
                }
                EvictionPolicy::LRU => {
                    // Simple LRU - remove oldest received
                    for _ in 0..count.min(pending_txs.len()) {
                        if let Some(evicted) = pending_txs.pop_front() {
                            tx_index.remove(&evicted.transaction.hash());
                            evicted_count += 1;
                        }
                    }
                }
            }
            
            evicted_count
        }; // Locks are dropped here
        
        // Update stats
        {
            let mut stats = self.stats.lock().await;
            stats.total_evicted += actual_evicted as u64;
        }
        
        Ok(())
    }
    
    async fn is_batch_ready(&self) -> bool {
        let current_size = self.pending_txs.read().len();
        current_size >= self.config.max_batch_size
    }
    
    async fn get_priority_batch(&self, max_size: usize) -> Result<Vec<Transaction>, HotStuffError> {
        let mut priority_queue = self.fee_priority_queue.write();
        let mut pending_txs = self.pending_txs.write();
        let mut tx_index = self.tx_index.write();
        
        let mut batch = Vec::new();
        let mut processed_hashes = Vec::new();
        
        while batch.len() < max_size && !priority_queue.is_empty() {
            if let Some(priority_tx) = priority_queue.pop() {
                if let Some(&index) = tx_index.get(&priority_tx.hash) {
                    if index < pending_txs.len() {
                        let pending_tx = &pending_txs[index];
                        batch.push(pending_tx.transaction.clone());
                        processed_hashes.push(priority_tx.hash);
                    }
                }
            }
        }
        
        // Remove processed transactions
        for hash in processed_hashes {
            if let Some(&index) = tx_index.get(&hash) {
                if index < pending_txs.len() {
                    pending_txs.remove(index);
                    tx_index.remove(&hash);
                    
                    // Update indices for remaining transactions
                    for (_existing_hash, existing_index) in tx_index.iter_mut() {
                        if *existing_index > index {
                            *existing_index -= 1;
                        }
                    }
                }
            }
        }
        
        Ok(batch)
    }
    
    async fn get_fifo_batch(&self, max_size: usize) -> Result<Vec<Transaction>, HotStuffError> {
        let mut pending_txs = self.pending_txs.write();
        let mut tx_index = self.tx_index.write();
        
        let batch_size = max_size.min(pending_txs.len());
        let mut batch = Vec::with_capacity(batch_size);
        
        for _ in 0..batch_size {
            if let Some(pending_tx) = pending_txs.pop_front() {
                tx_index.remove(&pending_tx.transaction.hash());
                batch.push(pending_tx.transaction);
            }
        }
        
        // Rebuild index
        tx_index.clear();
        for (i, pending_tx) in pending_txs.iter().enumerate() {
            tx_index.insert(pending_tx.transaction.hash(), i);
        }
        
        Ok(batch)
    }
    
    async fn cleanup_expired_transactions(&self) -> Result<(), HotStuffError> {
        let now = Instant::now();
        let mut expired_count = 0;
        
        // Scope the locks to avoid holding them across await
        {
            let mut pending_txs = self.pending_txs.write();
            let mut tx_index = self.tx_index.write();
            
            pending_txs.retain(|pending_tx| {
                let is_expired = now.duration_since(pending_tx.received_at) > self.config.max_tx_age;
                if is_expired {
                    tx_index.remove(&pending_tx.transaction.hash());
                    expired_count += 1;
                }
                !is_expired
            });
            
            // Rebuild index after cleanup
            tx_index.clear();
            for (i, pending_tx) in pending_txs.iter().enumerate() {
                tx_index.insert(pending_tx.transaction.hash(), i);
            }
        } // Locks are released here
        
        if expired_count > 0 {
            // Update stats - safe to await here since locks are released
            let mut stats = self.stats.lock().await;
            stats.total_evicted += expired_count;
            drop(stats); // Explicitly drop the lock
            
            info!("Cleaned up {} expired transactions", expired_count);
        }
        
        Ok(())
    }
    
    async fn recompute_priorities(&self) -> Result<(), HotStuffError> {
        if !self.config.enable_fee_prioritization {
            return Ok(());
        }
        
        let pending_txs = self.pending_txs.read();
        let mut priority_queue = self.fee_priority_queue.write();
        
        // Rebuild priority queue with updated scores
        priority_queue.clear();
        
        for pending_tx in pending_txs.iter() {
            let updated_score = self.calculate_priority_score(&pending_tx.transaction, pending_tx.fee);
            priority_queue.push(PrioritizedTx {
                hash: pending_tx.transaction.hash(),
                priority_score: updated_score,
                fee: pending_tx.fee,
                received_at: pending_tx.received_at,
            });
        }
        
        debug!("Recomputed priorities for {} transactions", pending_txs.len());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Transaction;
    
    #[tokio::test]
    async fn test_transaction_submission() {
        let config = TxPoolConfig::default();
        let pool = ProductionTxPool::new(config);
        
        let tx = Transaction {
            id: "test_tx_1".to_string(),
            data: b"test data".to_vec(),
        };
        
        assert!(pool.submit_transaction(tx).await.is_ok());
        
        let stats = pool.get_stats().await;
        assert_eq!(stats.total_received, 1);
        assert_eq!(stats.current_pool_size, 1);
    }
    
    #[tokio::test]
    async fn test_batch_creation() {
        let config = TxPoolConfig {
            max_batch_size: 5,
            min_batch_size: 2,
            ..Default::default()
        };
        let pool = ProductionTxPool::new(config);
        
        // Submit multiple transactions
        for i in 0..10 {
            let tx = Transaction {
                id: format!("test_tx_{}", i),
                data: format!("test data {}", i).into_bytes(),
            };
            pool.submit_transaction(tx).await.unwrap();
        }
        
        let batch = pool.get_next_batch(Some(5)).await.unwrap();
        assert_eq!(batch.len(), 5);
    }
    
    #[tokio::test]
    async fn test_fee_prioritization() {
        let config = TxPoolConfig {
            enable_fee_prioritization: true,
            ..Default::default()
        };
        let pool = ProductionTxPool::new(config);
        
        // Submit transactions with different implicit fees (based on size)
        let small_tx = Transaction {
            id: "small_tx".to_string(),
            data: b"small".to_vec(), // Lower fee
        };
        
        let large_tx = Transaction {
            id: "large_tx".to_string(),
            data: vec![0u8; 10000], // Higher fee due to size
        };
        
        pool.submit_transaction(small_tx).await.unwrap();
        pool.submit_transaction(large_tx).await.unwrap();
        
        let batch = pool.get_next_batch(Some(1)).await.unwrap();
        // Should get the large transaction first due to higher fee
        assert_eq!(batch[0].id, "large_tx");
    }
}

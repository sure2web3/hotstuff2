use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use crate::error::HotStuffError;

pub struct Mempool {
    transactions: RwLock<HashMap<Vec<u8>, Vec<u8>>>, // Hash -> Transaction
    max_size: usize,
}

impl Mempool {
    pub fn new(max_size: usize) -> Self {
        Self {
            transactions: RwLock::new(HashMap::new()),
            max_size,
        }
    }

    pub fn add_transaction(&self, tx: Vec<u8>) -> Result<(), HotStuffError> {
        let mut txs = self.transactions.write().unwrap();

        // Check if we're at max capacity
        if txs.len() >= self.max_size {
            return Err(HotStuffError::Storage("Mempool is full".to_string()));
        }

        // Use the hash of the transaction as the key
        let tx_hash = crate::types::Hash::from_bytes(&tx);
        txs.insert(tx_hash.as_bytes().to_vec(), tx);

        Ok(())
    }

    pub fn get_transaction(&self, tx_hash: &[u8]) -> Result<Option<Vec<u8>>, HotStuffError> {
        let txs = self.transactions.read().unwrap();
        Ok(txs.get(tx_hash).cloned())
    }

    pub fn remove_transactions(&self, tx_hashes: &[Vec<u8>]) -> Result<(), HotStuffError> {
        let mut txs = self.transactions.write().unwrap();
        for hash in tx_hashes {
            txs.remove(hash);
        }
        Ok(())
    }

    pub fn get_batch(&self, max_count: usize) -> Result<Vec<Vec<u8>>, HotStuffError> {
        let txs = self.transactions.read().unwrap();

        // Select up to max_count transactions
        let batch: Vec<_> = txs.values().take(max_count).cloned().collect();

        Ok(batch)
    }
}

use serde::{Deserialize, Serialize};

use crate::types::{Hash, Timestamp};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct QuorumCert {
    pub block_hash: Hash,
    pub height: u64,
    pub timestamp: Timestamp,
    pub signatures: Vec<(u64, Vec<u8>)>, // (node_id, signature)
}

impl QuorumCert {
    pub fn new(block_hash: Hash, height: u64, signatures: Vec<(u64, Vec<u8>)>) -> Self {
        Self {
            block_hash,
            height,
            timestamp: Timestamp::now(),
            signatures,
        }
    }

    pub fn verify(&self, block_hash: Hash, threshold: usize) -> bool {
        if self.block_hash != block_hash {
            return false;
        }

        // Check if we have enough signatures
        if self.signatures.len() < threshold {
            return false;
        }

        // TODO: Actually verify the signatures using public keys
        // This is a placeholder for the actual cryptographic verification
        true
    }
}

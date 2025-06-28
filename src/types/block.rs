use serde::{Deserialize, Serialize};
use std::fmt::{self, Formatter};

use crate::types::{Hash, Timestamp, Transaction};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Block {
    pub parent_hash: Hash,
    pub transactions: Vec<Transaction>,
    pub height: u64,
    pub proposer_id: u64,
    pub timestamp: Timestamp,
    pub hash: Hash,
}

impl Block {
    pub fn new(
        parent_hash: Hash,
        transactions: Vec<Transaction>,
        height: u64,
        proposer_id: u64,
    ) -> Self {
        let timestamp = Timestamp::now();
        let mut data = Vec::new();
        data.extend_from_slice(&parent_hash.as_bytes());
        data.extend_from_slice(&height.to_be_bytes());
        data.extend_from_slice(&proposer_id.to_be_bytes());
        data.extend_from_slice(&timestamp.as_u64().to_be_bytes());

        for tx in &transactions {
            data.extend_from_slice(&tx.data);
        }

        let hash = Hash::from_bytes(&data);

        Self {
            parent_hash,
            transactions,
            height,
            proposer_id,
            timestamp,
            hash,
        }
    }

    pub fn hash(&self) -> Hash {
        self.hash
    }
}

impl fmt::Display for Block {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Block {{ hash: {}, parent: {}, height: {}, proposer: {} }}",
            self.hash, self.parent_hash, self.height, self.proposer_id
        )
    }
}

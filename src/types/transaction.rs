use serde::{Deserialize, Serialize};
use crate::types::Hash;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transaction {
    pub id: String,
    pub data: Vec<u8>,
}

impl Transaction {
    pub fn new(id: String, data: Vec<u8>) -> Self {
        Transaction { id, data }
    }
    
    /// Calculate hash of the transaction
    pub fn hash(&self) -> Hash {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&self.id.as_bytes());
        hasher.update(&self.data);
        let result = hasher.finalize();
        Hash::from_bytes(&result[..])
    }
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transaction {
    pub id: String,
    pub data: Vec<u8>,
}

impl Transaction {
    pub fn new(id: String, data: Vec<u8>) -> Self {
        Transaction { id, data }
    }
}

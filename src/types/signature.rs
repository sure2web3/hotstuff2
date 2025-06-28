use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    pub signer: u64,
    pub data: Vec<u8>,
}

impl Signature {
    pub fn new(signer: u64, data: Vec<u8>) -> Self {
        Signature { signer, data }
    }
}

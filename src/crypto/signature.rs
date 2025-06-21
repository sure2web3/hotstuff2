use serde::{Deserialize, Serialize};

use crate::error::HotStuffError;
use crate::types::Hash;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Signature(Vec<u8>);

impl Signature {
    pub fn new(data: Vec<u8>) -> Self {
        Self(data)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn verify(
        &self,
        public_key: &crate::crypto::PublicKey,
        message: &[u8],
    ) -> Result<bool, HotStuffError> {
        public_key.verify(message, &self.0)
    }
}

pub trait Signable {
    fn bytes(&self) -> Vec<u8>;
}

impl Signable for Hash {
    fn bytes(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

use rand::{CryptoRng, RngCore};
use serde::{Deserialize, Serialize};

use crate::error::HotStuffError;
use sha2::Digest;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct KeyPair {
    private_key: Vec<u8>,
    public_key: PublicKey,
}

impl KeyPair {
    pub fn generate<R: RngCore + CryptoRng>(rng: &mut R) -> Self {
        // This is a placeholder for actual key generation
        // In a real implementation, use a proper cryptographic library
        let mut private_key = vec![0u8; 32];
        rng.fill_bytes(&mut private_key);

        // For simplicity, public key is just a hash of the private key
        let public_key = PublicKey(sha2::Sha256::digest(&private_key).into());

        Self {
            private_key,
            public_key,
        }
    }

    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, HotStuffError> {
        // This is a placeholder for actual signing
        // In a real implementation, use a proper cryptographic library
        let mut signature = vec![0u8; 64]; // Dummy signature
        for (i, &byte) in message.iter().enumerate() {
            if i < 32 {
                signature[i] = byte;
            }
        }
        for (i, &byte) in self.private_key.iter().enumerate() {
            if i < 32 {
                signature[32 + i] = byte;
            }
        }
        Ok(signature)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Debug)]
pub struct PublicKey(pub [u8; 32]);

impl PublicKey {
    pub fn verify(&self, _message: &[u8], signature: &[u8]) -> Result<bool, HotStuffError> {
        // This is a placeholder for actual signature verification
        // In a real implementation, use a proper cryptographic library
        Ok(signature.len() == 64) // Dummy verification
    }
}

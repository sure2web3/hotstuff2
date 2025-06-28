use serde::{Deserialize, Serialize};

use crate::types::{Hash, Timestamp, Signature};
use crate::crypto::bls_threshold::BlsSignature;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct QuorumCert {
    pub block_hash: Hash,
    pub height: u64,
    pub timestamp: Timestamp,
    pub signatures: Vec<Signature>, // Traditional signatures from different nodes
    pub threshold_signature: Option<BlsSignature>, // Efficient BLS threshold signature
}

impl QuorumCert {
    pub fn new(block_hash: Hash, height: u64, signatures: Vec<Signature>) -> Self {
        Self {
            block_hash,
            height,
            timestamp: Timestamp::now(),
            signatures,
            threshold_signature: None,
        }
    }

    pub fn new_with_threshold_sig(
        block_hash: Hash,
        height: u64,
        threshold_signature: BlsSignature,
    ) -> Self {
        Self {
            block_hash,
            height,
            timestamp: Timestamp::now(),
            signatures: Vec::new(), // Empty when using threshold signatures
            threshold_signature: Some(threshold_signature),
        }
    }

    pub fn verify(&self, block_hash: Hash, threshold: usize) -> bool {
        if self.block_hash != block_hash {
            return false;
        }

        // Verify threshold signature if present
        if let Some(ref _threshold_sig) = self.threshold_signature {
            // TODO: Implement proper BLS signature verification
            return true; // Placeholder - should verify against aggregate public key
        }

        // Otherwise, check traditional signatures
        if self.signatures.len() < threshold {
            return false;
        }

        // TODO: Actually verify the signatures using public keys
        // This is a placeholder for the actual cryptographic verification
        true
    }

    /// Check if this QC uses the more efficient threshold signature
    pub fn uses_threshold_signature(&self) -> bool {
        self.threshold_signature.is_some()
    }

    /// Get the number of signers (works for both traditional and threshold signatures)
    pub fn signer_count(&self) -> usize {
        if self.threshold_signature.is_some() {
            // For BLS threshold signatures, we don't track individual signers in the same way
            // Return the threshold as an approximation
            self.signatures.len().max(1)
        } else {
            self.signatures.len()
        }
    }
}

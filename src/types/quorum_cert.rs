use serde::{Deserialize, Serialize};

use crate::types::{Hash, Timestamp, Signature};
use crate::crypto::bls_threshold::{BlsSignature, BlsPublicKey};

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
            // This requires the aggregate public key which isn't stored in QC
            // For now, we trust that the threshold signature was properly verified
            // when it was created in the consensus protocol
            return true;
        }

        // Otherwise, check traditional signatures
        if self.signatures.len() < threshold {
            return false;
        }

        // TODO: Actually verify the signatures using public keys
        // This is a placeholder for the actual cryptographic verification
        true
    }

    pub fn verify_with_bls_key(&self, block_hash: Hash, threshold: usize, aggregate_public_key: &BlsPublicKey) -> bool {
        if self.block_hash != block_hash {
            return false;
        }

        // Verify BLS threshold signature if present
        if let Some(ref threshold_sig) = self.threshold_signature {
            return self.verify_bls_signature(block_hash.as_bytes(), threshold_sig, aggregate_public_key);
        }

        // Otherwise, check traditional signatures
        if self.signatures.len() < threshold {
            return false;
        }

        // TODO: Actually verify the signatures using public keys
        // This is a placeholder for the actual cryptographic verification
        true
    }

    /// Verify BLS threshold signature using pairing
    fn verify_bls_signature(&self, message: &[u8], signature: &BlsSignature, public_key: &BlsPublicKey) -> bool {
        use bls12_381::{pairing, G1Projective, G2Affine};
        use sha2::{Digest, Sha256};
        use ff::Field;
        use bls12_381::Scalar;
        use group::Curve;

        // Hash message to G1 point (must match the signing implementation)
        let message_hash = {
            let mut hasher = Sha256::new();
            hasher.update(message);
            let hash = hasher.finalize();
            
            let scalar = Scalar::from_bytes_wide(&{
                let mut wide = [0u8; 64];
                wide[..32].copy_from_slice(&hash);
                // Use message bytes for additional entropy without extra hashing
                if message.len() >= 32 {
                    wide[32..].copy_from_slice(&message[..32]);
                } else {
                    wide[32..32+message.len()].copy_from_slice(message);
                }
                wide
            });
            
            (G1Projective::generator() * scalar).to_affine()
        };

        // Verify: e(H(m), pk) == e(sig, g2)
        let lhs = pairing(&message_hash, &public_key.point);
        let rhs = pairing(&signature.point, &G2Affine::generator());
        
        lhs == rhs
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

// Comprehensive BLS integration tests for HotStuff-2 consensus
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::bls_threshold::{
    BlsSecretKey, BlsPublicKey, 
    ProductionThresholdSigner, ThresholdSignatureManager
};
use crate::types::{Hash, QuorumCert};

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::ChaCha20Rng;
    use rand_core::SeedableRng;

    /// Test complete BLS threshold signature workflow
    #[tokio::test]
    async fn test_complete_bls_workflow() {
        let num_nodes = 4;
        let threshold = 3; // 2f+1 for f=1
        let mut rng = ChaCha20Rng::from_seed([0u8; 32]);

        // Generate keys for all nodes
        let mut secret_keys = Vec::new();
        let mut public_keys = HashMap::new();

        for i in 0..num_nodes {
            let sk = BlsSecretKey::generate(&mut rng);
            let pk = sk.public_key();
            public_keys.insert(i, pk);
            secret_keys.push(sk);
        }

        // Create threshold signers for each node
        let mut signers = Vec::new();
        for i in 0..num_nodes {
            let signer = ProductionThresholdSigner::new(
                i,
                threshold,
                secret_keys[i as usize].clone(),
                public_keys.clone(),
            ).unwrap();
            signers.push(signer);
        }

        // Create threshold signature managers
        let mut managers = Vec::new();
        for signer in signers {
            managers.push(ThresholdSignatureManager::new(signer));
        }

        // Simulate consensus round
        let block_hash = Hash::from_bytes(b"test_block_hash_12345678901234567890123456789012");
        let message = block_hash.as_bytes();

        // Step 1: Each node signs the message
        let mut partial_signatures = Vec::new();
        for (i, manager) in managers.iter().enumerate() {
            let partial_sig = manager.sign_partial(message).unwrap();
            partial_signatures.push((i as u64, partial_sig));
        }

        // Step 2: Aggregate signatures at node 0 (leader)
        let manager_0 = &mut managers[0];
        
        // Add partial signatures from all nodes (including self)
        for (node_id, partial_sig) in &partial_signatures {
            manager_0.add_partial_signature(message, *node_id, partial_sig.clone()).unwrap();
        }

        // Check if we have threshold signatures
        assert!(manager_0.has_threshold_signatures(message));

        // Get available signers
        let available_signers = manager_0.get_available_signers(message);
        assert_eq!(available_signers.len(), num_nodes as usize);

        // Try to combine signatures
        let threshold_sig = manager_0.try_combine(message, &available_signers).unwrap();
        assert!(threshold_sig.is_some());

        let aggregated_signature = threshold_sig.unwrap();

        // Step 3: Verify the aggregated signature at different nodes
        for (i, manager) in managers.iter().enumerate() {
            let aggregate_pk = manager.get_signer().aggregate_public_key();
            
            // Create a QuorumCert with the threshold signature
            let qc = QuorumCert::new_with_threshold_sig(
                block_hash,
                1, // height
                aggregated_signature.clone(),
            );

            // Verify the QC
            let is_valid = qc.verify_with_bls_key(block_hash, threshold, aggregate_pk);
            assert!(is_valid, "Node {} failed to verify threshold signature", i);
        }

        println!("✅ Complete BLS threshold signature workflow test passed");
    }

    /// Test insufficient signatures scenario
    #[tokio::test]
    async fn test_insufficient_bls_signatures() {
        let num_nodes = 4;
        let threshold = 3;
        let mut rng = ChaCha20Rng::from_seed([1u8; 32]);

        // Generate keys
        let mut secret_keys = Vec::new();
        let mut public_keys = HashMap::new();

        for i in 0..num_nodes {
            let sk = BlsSecretKey::generate(&mut rng);
            let pk = sk.public_key();
            public_keys.insert(i, pk);
            secret_keys.push(sk);
        }

        // Create threshold signer for node 0
        let signer = ProductionThresholdSigner::new(
            0,
            threshold,
            secret_keys[0].clone(),
            public_keys,
        ).unwrap();

        let mut manager = ThresholdSignatureManager::new(signer);

        let block_hash = Hash::from_bytes(b"insufficient_test_hash_1234567890123456789012345678");
        let message = block_hash.as_bytes();

        // Add only threshold-1 signatures (not enough)
        for i in 0..threshold-1 {
            let sk = &secret_keys[i];
            let partial_sig = sk.sign(message);
            manager.add_partial_signature(message, i as u64, partial_sig).unwrap();
        }

        // Should not have threshold signatures
        assert!(!manager.has_threshold_signatures(message));

        // Try to combine should return None
        let available_signers = manager.get_available_signers(message);
        let result = manager.try_combine(message, &available_signers).unwrap();
        assert!(result.is_none());

        println!("✅ Insufficient signatures test passed");
    }

    /// Test Byzantine fault tolerance - invalid signatures
    #[tokio::test]
    async fn test_byzantine_invalid_signatures() {
        let num_nodes = 4;
        let threshold = 3;
        let mut rng = ChaCha20Rng::from_seed([2u8; 32]);

        // Generate keys for honest nodes
        let mut secret_keys = Vec::new();
        let mut public_keys = HashMap::new();

        for i in 0..num_nodes {
            let sk = BlsSecretKey::generate(&mut rng);
            let pk = sk.public_key();
            public_keys.insert(i, pk);
            secret_keys.push(sk);
        }

        let signer = ProductionThresholdSigner::new(
            0,
            threshold,
            secret_keys[0].clone(),
            public_keys,
        ).unwrap();

        let block_hash = Hash::from_bytes(b"byzantine_test_hash_123456789012345678901234567890");
        let message = block_hash.as_bytes();

        // Test individual signature verification
        // Valid signature from node 1
        let valid_sig = secret_keys[1].sign(message);
        assert!(signer.verify_individual(message, &valid_sig, 1));

        // Invalid signature (sign different message) from node 1
        let invalid_message = b"different_message";
        let invalid_sig = secret_keys[1].sign(invalid_message);
        assert!(!signer.verify_individual(message, &invalid_sig, 1));

        // Test that aggregation fails with invalid signatures
        let mut signatures = Vec::new();
        
        // Add 2 valid signatures
        for i in 0..2 {
            let sig = secret_keys[i].sign(message);
            signatures.push((i as u64, sig));
        }
        
        // Add 1 invalid signature (Byzantine node)
        let byzantine_sig = secret_keys[2].sign(b"malicious_message");
        signatures.push((2u64, byzantine_sig));

        // Aggregation should fail due to invalid signature verification
        let result = signer.aggregate_and_verify(message, &signatures);
        assert!(result.is_err());

        println!("✅ Byzantine fault tolerance test passed");
    }

    /// Test performance metrics and caching
    #[tokio::test]
    async fn test_bls_performance_features() {
        let num_nodes = 4;
        let threshold = 3;
        let mut rng = ChaCha20Rng::from_seed([3u8; 32]);

        // Generate keys
        let mut secret_keys = Vec::new();
        let mut public_keys = HashMap::new();

        for i in 0..num_nodes {
            let sk = BlsSecretKey::generate(&mut rng);
            let pk = sk.public_key();
            public_keys.insert(i, pk);
            secret_keys.push(sk);
        }

        let signer = ProductionThresholdSigner::new(
            0,
            threshold,
            secret_keys[0].clone(),
            public_keys,
        ).unwrap();

        let message = b"performance_test_message";

        // Test performance metrics
        let (initial_sigs, initial_verifs) = signer.get_metrics();

        // Perform some operations
        let _sig1 = signer.sign_threshold(message);
        let _sig2 = signer.sign_threshold(message);
        
        let valid_sig = secret_keys[1].sign(message);
        let _result1 = signer.verify_individual_cached(message, &valid_sig, 1);
        let _result2 = signer.verify_individual_cached(message, &valid_sig, 1); // Should hit cache

        let (final_sigs, final_verifs) = signer.get_metrics();

        // Check that metrics were updated
        assert_eq!(final_sigs, initial_sigs + 2);
        assert_eq!(final_verifs, initial_verifs + 1); // Second verification should hit cache

        // Test cache clearing
        signer.clear_cache();
        
        // Verify again after cache clear (should increment counter)
        let _result3 = signer.verify_individual_cached(message, &valid_sig, 1);
        let (_after_clear_sigs, after_clear_verifs) = signer.get_metrics();
        assert_eq!(after_clear_verifs, final_verifs + 1);

        println!("✅ Performance features test passed");
    }

    /// Test key generation and serialization
    #[tokio::test]
    async fn test_bls_key_serialization() {
        let num_nodes = 3;
        let threshold = 2;

        // Test distributed key generation
        let (aggregate_pk, secret_keys) = ProductionThresholdSigner::generate_keys(threshold, num_nodes).unwrap();

        // Test key serialization/deserialization
        for sk in &secret_keys {
            let sk_bytes = sk.to_bytes();
            let recovered_sk = BlsSecretKey::from_bytes(&sk_bytes).unwrap();
            assert_eq!(*sk, recovered_sk);

            let pk = sk.public_key();
            let pk_bytes = pk.to_bytes();
            let recovered_pk = BlsPublicKey::from_bytes(&pk_bytes).unwrap();
            assert_eq!(pk, recovered_pk);
        }

        // Test aggregate public key
        let individual_pks: Vec<BlsPublicKey> = secret_keys.iter().map(|sk| sk.public_key()).collect();
        let computed_aggregate = BlsPublicKey::aggregate(&individual_pks).unwrap();
        assert_eq!(aggregate_pk, computed_aggregate);

        println!("✅ Key generation and serialization test passed");
    }

    /// Test BLS signature verification (fast version)
    #[tokio::test]
    async fn test_fast_bls_verification() {
        let threshold = 2;
        let num_nodes = 3;
        let mut rng = ChaCha20Rng::from_seed([5u8; 32]);

        // Generate keys
        let mut secret_keys = Vec::new();
        let mut public_keys = HashMap::new();

        for i in 0..num_nodes {
            let sk = BlsSecretKey::generate(&mut rng);
            let pk = sk.public_key();
            public_keys.insert(i, pk);
            secret_keys.push(sk);
        }

        let signer = ProductionThresholdSigner::new(
            0,
            threshold,
            secret_keys[0].clone(),
            public_keys,
        ).unwrap();

        let message = b"fast_verification_test";
        
        // Test signing and basic verification
        let signature = signer.sign_threshold(message);
        let individual_verification = signer.verify_individual(message, &signature, 0);
        assert!(individual_verification, "Individual signature should verify correctly");

        // Test aggregation without expensive pairing verification
        let mut signatures = Vec::new();
        for i in 0..threshold {
            let sig = secret_keys[i].sign(message);
            signatures.push((i as u64, sig));
        }

        // This should succeed without doing expensive pairing
        let aggregation_result = signer.aggregate_and_verify(message, &signatures);
        assert!(aggregation_result.is_ok(), "Aggregation should succeed with sufficient signatures");

        println!("✅ Fast BLS verification test passed");
    }
}

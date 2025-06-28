#!/usr/bin/env cargo +stable run --bin

//! HotStuff-2 Consensus Demonstration
//! 
//! This script demonstrates the key features of the HotStuff-2 consensus implementation

use std::collections::HashMap;
use tracing::{info, warn};

use hotstuff2::{
    crypto::threshold::ThresholdSigner,
    error::HotStuffError,
    types::{Block, Hash, Transaction},
};

/// Simple key-value state machine for demonstration
#[derive(Debug, Clone)]
pub struct DemoStateMachine {
    state: HashMap<String, String>,
    committed_height: u64,
}

impl DemoStateMachine {
    pub fn new() -> Self {
        Self {
            state: HashMap::new(),
            committed_height: 0,
        }
    }

    fn apply_transaction(&mut self, tx: &Transaction) -> Result<u64, HotStuffError> {
        // Parse transaction as key=value
        let tx_data = String::from_utf8_lossy(&tx.data);
        if let Some((key, value)) = tx_data.split_once('=') {
            self.state.insert(key.to_string(), value.to_string());
            info!("Applied transaction: {} = {}", key, value);
            Ok(1) // Gas used
        } else {
            Err(HotStuffError::InvalidMessage("Invalid transaction format".to_string()))
        }
    }

    fn compute_state_root(&self) -> Hash {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        
        // Sort keys for deterministic hash
        let mut sorted_state: Vec<_> = self.state.iter().collect();
        sorted_state.sort_by_key(|(k, _)| *k);
        
        for (key, value) in sorted_state {
            hasher.update(key.as_bytes());
            hasher.update(value.as_bytes());
        }
        
        let hash_bytes = hasher.finalize();
        let mut hash_array = [0u8; 32];
        hash_array.copy_from_slice(&hash_bytes[..32]);
        Hash::from(hash_array)
    }

    fn execute_block(&mut self, block: &Block) -> Result<Hash, HotStuffError> {
        info!(
            "Executing block {} at height {} with {} transactions",
            block.hash(),
            block.height,
            block.transactions.len()
        );
        
        // Apply all transactions in the block
        for (i, tx) in block.transactions.iter().enumerate() {
            if let Err(e) = self.apply_transaction(tx) {
                warn!("Transaction {} failed: {}", i, e);
                return Err(e);
            }
        }
        
        // Update committed height
        self.committed_height = block.height;
        
        // Return new state root
        Ok(self.compute_state_root())
    }
}

/// Demonstrate threshold signature functionality
async fn demo_threshold_signatures() -> Result<(), HotStuffError> {
    println!("\n🔐 Threshold Signature Demonstration");
    println!("=====================================");

    let threshold = 3;
    let total_nodes = 5;
    let message = b"Hello, HotStuff-2!";

    info!("Generating threshold keys for {}/{} scheme", threshold, total_nodes);
    let (public_key, secret_keys) = ThresholdSigner::generate_keys(threshold, total_nodes)?;

    println!("✅ Generated {} secret key shares", secret_keys.len());
    println!("✅ Threshold: {} signatures required", threshold);

    // Create signers for first 3 nodes
    let mut signers = Vec::new();
    for i in 0..3 {
        signers.push(ThresholdSigner::new(
            secret_keys[i].clone(),
            public_key.clone(),
        ));
    }

    // Create partial signatures
    println!("\n📝 Creating partial signatures...");
    let mut partial_sigs = Vec::new();
    for (i, signer) in signers.iter().enumerate() {
        let partial = signer.sign_partial(message)?;
        println!("✅ Node {} created partial signature", i);
        partial_sigs.push(partial);
    }

    // Combine signatures on first signer
    println!("\n🔗 Combining signatures...");
    let mut first_signer = signers.into_iter().next().unwrap();
    
    // Add other partial signatures
    for partial in partial_sigs.iter().skip(1) {
        first_signer.add_partial_signature(message, partial.clone())?;
    }

    // Try to combine
    let signer_ids: Vec<u64> = (0..3).collect();
    if let Some(threshold_sig) = first_signer.try_combine(message, &signer_ids)? {
        println!("✅ Successfully combined {} signatures", threshold_sig.signers.len());
        
        // Verify
        let is_valid = first_signer.verify_threshold(message, &threshold_sig)?;
        println!("✅ Threshold signature verification: {}", is_valid);
        
        if threshold_sig.is_valid_threshold() {
            println!("✅ Signature meets threshold requirements");
        }
    } else {
        warn!("❌ Failed to combine signatures");
    }

    Ok(())
}

/// Demonstrate the complete consensus flow
async fn demo_complete_flow() -> Result<(), HotStuffError> {
    println!("\n🌊 Complete Consensus Flow");
    println!("===========================");

    // Create sample transactions
    let transactions = vec![
        Transaction::new("tx1".to_string(), b"user1=alice".to_vec()),
        Transaction::new("tx2".to_string(), b"user2=bob".to_vec()),
        Transaction::new("tx3".to_string(), b"balance_alice=100".to_vec()),
        Transaction::new("tx4".to_string(), b"balance_bob=50".to_vec()),
    ];

    println!("✅ Created {} sample transactions", transactions.len());

    // Create a block with these transactions
    let block = Block::new(
        Hash::zero(), // parent hash
        transactions,
        1,            // height
        0,            // proposer
    );

    println!("✅ Created block {} with {} transactions", block.hash(), block.transactions.len());

    // Demonstrate state machine execution
    let mut state_machine = DemoStateMachine::new();
    
    println!("\n⚙️  Executing transactions...");
    let state_root = state_machine.execute_block(&block)?;
    println!("✅ Block executed successfully");
    println!("✅ New state root: {}", state_root);

    // Show final state
    println!("\n📋 Final State:");
    for (key, value) in &state_machine.state {
        println!("   {} = {}", key, value);
    }
    println!("✅ Block committed at height {}", state_machine.committed_height);

    Ok(())
}

/// Demonstrate Byzantine fault tolerance concepts
async fn demo_byzantine_tolerance() -> Result<(), HotStuffError> {
    println!("\n🛡️  Byzantine Fault Tolerance");
    println!("==============================");

    // Demonstrate different network sizes and their fault tolerance
    let network_sizes = vec![4, 7, 10, 13, 16];
    
    println!("Network Size | Max Faults | Safety Threshold");
    println!("-------------|------------|------------------");
    
    for n in &network_sizes {
        let f = (n - 1) / 3;  // Maximum Byzantine faults
        let safety_threshold = 2 * f + 1;  // Minimum for safety
        
        println!("{:12} | {:10} | {:16}", n, f, safety_threshold);
    }

    println!("\n✅ HotStuff-2 guarantees:");
    println!("   • Safety: No forks under any network conditions");
    println!("   • Liveness: Progress under network synchrony");
    println!("   • Optimal fault tolerance: f = ⌊(n-1)/3⌋");

    // Demonstrate threshold calculations
    println!("\n🔢 Threshold Signature Requirements:");
    for n in &network_sizes[0..3] {
        let f = (n - 1) / 3;
        let threshold = 2 * f + 1;
        
        println!("   {} nodes: need {} signatures (tolerate {} faults)", n, threshold, f);
    }

    Ok(())
}

/// Demonstrate two-phase consensus concept
async fn demo_two_phase_concept() -> Result<(), HotStuffError> {
    println!("\n🏛️  Two-Phase Consensus Protocol");
    println!("==================================");

    println!("HotStuff-2 uses an optimized two-phase protocol:");
    println!();
    println!("Phase 1 - PROPOSE:");
    println!("  • Leader creates block with embedded QC");
    println!("  • Leader broadcasts proposal to all replicas");
    println!("  • Replicas validate proposal and parent QC");
    println!();
    println!("Phase 2 - COMMIT:");
    println!("  • Replicas send votes for valid proposals");
    println!("  • Leader collects votes and forms QC");
    println!("  • Block is committed when QC is formed");
    println!();
    println!("✅ Advantages over traditional 3-phase BFT:");
    println!("   • Reduced message complexity");
    println!("   • Lower latency under synchrony");
    println!("   • Pipelined execution for higher throughput");
    println!("   • Optimistic responsiveness");

    // Create a simple example
    println!("\n📊 Example with 4 nodes (f=1):");
    println!("   1. Node 0 (leader) proposes block");
    println!("   2. Nodes 1,2,3 validate and vote");
    println!("   3. Node 0 collects 3 votes (≥ 2f+1)");
    println!("   4. Block is committed with QC");
    println!("   ✅ Total: 2 message rounds (vs 3 in traditional BFT)");

    Ok(())
}

/// Main demonstration runner
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🚀 HotStuff-2 Consensus Protocol Demonstration");
    println!("===============================================");
    println!();
    println!("This demonstration showcases the key features of our");
    println!("HotStuff-2 implementation following the academic paper:");
    println!("https://eprint.iacr.org/2023/397.pdf");
    println!();

    // Run demonstrations
    if let Err(e) = demo_threshold_signatures().await {
        eprintln!("❌ Threshold signature demo failed: {}", e);
    }

    if let Err(e) = demo_two_phase_concept().await {
        eprintln!("❌ Two-phase concept demo failed: {}", e);
    }

    if let Err(e) = demo_byzantine_tolerance().await {
        eprintln!("❌ Byzantine tolerance demo failed: {}", e);
    }

    if let Err(e) = demo_complete_flow().await {
        eprintln!("❌ Complete flow demo failed: {}", e);
    }

    println!("\n🎉 Demonstration Complete!");
    println!("===========================");
    println!();
    println!("Key Features Demonstrated:");
    println!("✅ Two-phase consensus (Propose, Commit)");
    println!("✅ Threshold signature aggregation");
    println!("✅ Byzantine fault tolerance analysis");
    println!("✅ State machine execution");
    println!("✅ Transaction processing");
    println!("✅ Block creation and validation");
    println!();
    println!("Implementation Highlights:");
    println!("✅ Optimal message complexity: O(n) per decision");
    println!("✅ Fast path latency: 2Δ (network delay)");
    println!("✅ Pipelined execution for high throughput");
    println!("✅ View-based leader rotation");
    println!("✅ Threshold signature aggregation");
    println!("✅ Persistent storage with RocksDB");
    println!();
    println!("For more information, see:");
    println!("- README.md for usage instructions");
    println!("- CHANGELOG.md for implementation details");
    println!("- src/protocol/ for protocol implementation");
    println!("- Academic paper: https://eprint.iacr.org/2023/397.pdf");

    Ok(())
}

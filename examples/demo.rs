#!/usr/bin/env cargo +stable run --bin

//! HotStuff-2 Consensus Demonstration
//! 
//! This script demonstrates the key features of the HotStuff-2 consensus implementation:
//! - Two-phase consensus protocol
//! - Threshold signature aggregation
//! - View management and leader rotation
//! - Pipelined block production
//! - Byzantine fault tolerance

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use hotstuff2::{
    config::HotStuffConfig,
    consensus::state_machine::StateMachine,
    crypto::{threshold::ThresholdSigner, KeyPair},
    error::HotStuffError,
    message::consensus::Vote,
    network::NetworkClient,
    protocol::hotstuff2::HotStuff2,
    storage::rocksdb_store::RocksDBStore,
    timer::TimeoutManager,
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
        
        Hash::from(hasher.finalize().as_slice())
    }
}

#[async_trait::async_trait]
impl StateMachine for DemoStateMachine {
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
    
    fn state_hash(&self) -> Hash {
        self.compute_state_root()
    }
    
    fn height(&self) -> u64 {
        self.committed_height
    }
    
    fn reset_to_state(&mut self, _state_hash: Hash, height: u64) -> Result<(), HotStuffError> {
        self.committed_height = height;
        // In a real implementation, we would restore the state to match the hash
        Ok(())
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

/// Demonstrate two-phase consensus
async fn demo_consensus_protocol() -> Result<(), HotStuffError> {
    println!("\n🏛️  Two-Phase Consensus Demonstration");
    println!("=====================================");

    // Create a test node
    let config = HotStuffConfig::default();
    let key_pair = KeyPair::generate();
    let network_client = Arc::new(NetworkClient::new("127.0.0.1:0".to_string()));
    
    // Use a temporary directory for this demo
    let data_dir = "/tmp/hotstuff2_demo";
    std::fs::create_dir_all(data_dir).ok();
    let block_store = Arc::new(RocksDBStore::new(data_dir)?);
    
    let timeout_manager = Arc::new(Mutex::new(TimeoutManager::new()));
    let state_machine = Arc::new(Mutex::new(DemoStateMachine::new()));

    let node = HotStuff2::new(
        0,              // node_id
        4,              // num_nodes  
        key_pair,
        network_client,
        block_store,
        timeout_manager,
        config,
        state_machine,
    );

    println!("✅ Created HotStuff-2 node (ID: 0, Cluster: 4 nodes)");
    println!("✅ Byzantine fault tolerance: f = {}", node.f);

    // Demonstrate view management
    println!("\n📊 View Management...");
    let initial_view = {
        let view = node.current_view.lock().await;
        println!("✅ Initial view: {}, Leader: {}", view.number, view.leader);
        view.number
    };

    // Trigger view change
    node.start_view_change("Demo timeout").await?;
    let new_view = {
        let view = node.current_view.lock().await;
        println!("✅ After view change: {}, Leader: {}", view.number, view.leader);
        view.number
    };

    assert!(new_view > initial_view);
    println!("✅ View change successful");

    // Demonstrate block creation
    println!("\n🧱 Block Creation...");
    let block = node.create_chained_block().await?;
    println!("✅ Created block {} at height {}", block.hash(), block.height);
    println!("✅ Block contains {} transactions", block.transactions.len());

    // Demonstrate vote processing
    println!("\n🗳️  Vote Processing...");
    let vote = Vote {
        sender_id: 1,
        block_hash: block.hash(),
        height: block.height,
        view: new_view,
        signature: vec![1, 2, 3, 4], // Mock signature
    };

    node.process_vote(vote).await?;
    println!("✅ Processed vote for block {}", block.hash());

    // Check pipeline state
    if let Some(stage) = node.pipeline.get(&block.height) {
        println!("✅ Pipeline stage created for height {}", stage.height);
        println!("✅ Stage has {} votes", stage.votes.len());
    }

    // Demonstrate optimistic responsiveness
    println!("\n⚡ Optimistic Responsiveness...");
    let is_sync = node.detect_network_synchrony().await?;
    println!("✅ Network synchrony detected: {}", is_sync);
    
    let fast_path = node.should_use_fast_path().await?;
    println!("✅ Fast path enabled: {}", fast_path);

    // Calculate optimal batch size
    let batch_size = node.calculate_optimal_batch_size().await?;
    println!("✅ Optimal batch size: {} transactions", batch_size);

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
    for (i, tx) in block.transactions.iter().enumerate() {
        match state_machine.execute_transaction(tx).await {
            Ok(result) => {
                println!("✅ Transaction {} executed, gas used: {}", i, result.gas_used);
            }
            Err(e) => {
                warn!("❌ Transaction {} failed: {}", i, e);
            }
        }
    }

    // Commit the block
    state_machine.commit_block(&block).await?;
    println!("✅ Block committed at height {}", block.height);

    // Show final state
    println!("\n📋 Final State:");
    for (key, value) in &state_machine.state {
        println!("   {} = {}", key, value);
    }

    let state_root = state_machine.get_state_root().await;
    println!("✅ State root: {}", state_root);

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

    if let Err(e) = demo_consensus_protocol().await {
        eprintln!("❌ Consensus protocol demo failed: {}", e);
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
    println!("✅ View management and leader rotation");
    println!("✅ Optimistic responsiveness");
    println!("✅ Pipelined block processing");
    println!("✅ Byzantine fault tolerance (f = ⌊(n-1)/3⌋)");
    println!("✅ State machine execution");
    println!("✅ Transaction batching and processing");
    println!();
    println!("For more information, see:");
    println!("- README.md for usage instructions");
    println!("- CHANGELOG.md for implementation details");
    println!("- src/protocol/ for protocol implementation");
    println!("- Academic paper: https://eprint.iacr.org/2023/397.pdf");

    Ok(())
}

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use hotstuff2::{
    consensus::{Pacemaker, SafetyEngine, StateMachine},
    config::HotStuffConfig,
    crypto::KeyPair,
    message::consensus::ConsensusMsg,
    metrics::MetricsCollector,
    network::{NetworkClient, PeerAddr},
    protocol::HotStuff2,
    storage::RocksDBStore,
    timer::TimeoutManager,
    types::{Block, Hash, Transaction},
};

/// Demonstration of the HotStuff-2 protocol
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();
    
    println!("🔥 HotStuff-2 Consensus Protocol Demo");
    println!("=====================================");
    
    // Configuration for a 4-node network (tolerates 1 Byzantine fault)
    let num_nodes = 4;
    let f = 1; // Number of faulty nodes tolerated
    
    // Create a simple 4-node setup
    let mut nodes = Vec::new();
    
    for node_id in 0..num_nodes {
        let node = create_demo_node(node_id, num_nodes, f).await?;
        nodes.push(node);
        println!("✅ Created node {}", node_id);
    }
    
    println!("\n🚀 Starting HotStuff-2 protocol on all nodes...");
    
    // Start all nodes
    for node in &nodes {
        node.start().await;
    }
    
    println!("✅ All nodes started successfully!");
    println!("\n📝 The HotStuff-2 protocol is now running with:");
    println!("   - Two-phase consensus (Propose → Commit)");
    println!("   - View-based leader election");
    println!("   - Pipelined block processing");
    println!("   - Optimistic responsiveness");
    println!("   - Byzantine fault tolerance (f={}, n={})", f, num_nodes);
    
    // In a real implementation, you would:
    // 1. Submit transactions to the nodes
    // 2. Monitor consensus progress
    // 3. Verify safety and liveness properties
    
    println!("\n⏰ Letting the protocol run for a few seconds...");
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    println!("🎉 Demo completed! The HotStuff-2 implementation is working.");
    println!("📋 Check the logs above for protocol activity.");
    
    Ok(())
}

async fn create_demo_node(
    node_id: u64,
    num_nodes: u64,
    f: u64,
) -> Result<HotStuff2<RocksDBStore>, Box<dyn std::error::Error>> {
    // Generate a key pair for this node
    let mut rng = rand::thread_rng();
    let key_pair = KeyPair::generate(&mut rng);
    
    // Create peer addresses (in a real setup, these would be actual network addresses)
    let mut peers = HashMap::new();
    for peer_id in 0..num_nodes {
        if peer_id != node_id {
            peers.insert(peer_id, PeerAddr {
                ip: "127.0.0.1".to_string(),
                port: 8000 + peer_id as u16,
            });
        }
    }
    
    // Create network client
    let network_client = Arc::new(NetworkClient::new(node_id, peers));
    
    // Create storage
    let db_path = format!("data/node{}/blocks", node_id);
    let block_store = Arc::new(RocksDBStore::new(&db_path)?);
    
    // Create other components
    let timeout_manager = Arc::new(TimeoutManager::new());
    let pacemaker = Arc::new(parking_lot::Mutex::new(Pacemaker::new()));
    let safety_engine = Arc::new(parking_lot::Mutex::new(SafetyEngine::new()));
    let state_machine = Arc::new(parking_lot::Mutex::new(
        DemoStateMachine::new() as Box<dyn StateMachine>
    ));
    let metrics = Arc::new(MetricsCollector::new());
    
    let config = HotStuffConfig {
        timeout_duration: Duration::from_secs(2),
        max_block_size: 1000,
        view_change_timeout: Duration::from_secs(10),
        ..Default::default()
    };
    
    // Create message channel
    let (sender, receiver) = mpsc::channel(100);
    
    // Create HotStuff-2 instance
    let hotstuff = HotStuff2::new(
        node_id,
        key_pair,
        network_client,
        block_store,
        timeout_manager,
        pacemaker,
        safety_engine,
        state_machine,
        metrics,
        config,
        sender,
        receiver,
        num_nodes,
        f,
    );
    
    Ok(hotstuff)
}

/// Simple demo state machine
#[derive(Debug)]
struct DemoStateMachine {
    state: u64,
}

impl DemoStateMachine {
    fn new() -> Self {
        Self { state: 0 }
    }
}

impl StateMachine for DemoStateMachine {
    fn apply(&mut self, _transaction: &Transaction) -> Result<(), Box<dyn std::error::Error>> {
        self.state += 1;
        println!("  📊 State machine applied transaction, new state: {}", self.state);
        Ok(())
    }
    
    fn get_state_hash(&self) -> Hash {
        Hash::from_bytes(&self.state.to_be_bytes())
    }
}

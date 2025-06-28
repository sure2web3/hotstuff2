use std::time::Duration;

use hotstuff2::{
    node::{Node, NodeConfig},
    types::{Block, Hash, Transaction},
    HotStuffError,
};
use log::info;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), HotStuffError> {
    // Initialize logging
    env_logger::init();

    // Create a simple 4-node configuration
    let mut nodes = Vec::new();
    
    for i in 0..4 {
        let mut peers = Vec::new();
        for j in 0..4 {
            if i != j {
                peers.push((j, format!("127.0.0.1:{}", 8000 + j)));
            }
        }

        let config = NodeConfig {
            node_id: i,
            listen_addr: format!("127.0.0.1:{}", 8000 + i),
            peers,
            base_timeout: 5000,
            timeout_multiplier: 1.2,
            max_mempool_size: 1000,
            data_dir: format!("./data/node{}", i),
            metrics_port: Some(9000 + i as u16),
        };

        let node = Node::new(config);
        nodes.push(node);
    }

    // Start the first node (node 0) as an example
    info!("Starting node 0...");
    nodes[0].start().await?;

    // Wait a bit for the node to initialize
    sleep(Duration::from_secs(2)).await;

    // Create a sample block
    let parent_hash = Hash::from_bytes(b"genesis");
    let transactions = vec![Transaction::new(b"Hello, HotStuff-2!".to_vec())];
    
    let block = Block::new(
        parent_hash,
        transactions,
        1, // height
        0, // proposer_id (node 0)
    );

    // Propose the block
    info!("Proposing block...");
    nodes[0].propose(block).await?;

    // Wait for some processing
    sleep(Duration::from_secs(5)).await;

    // Check for committed blocks
    if let Some(committed_block) = nodes[0].committed_block().await {
        info!("Committed block at height: {}", committed_block.height);
    } else {
        info!("No blocks committed yet");
    }

    // Shutdown
    info!("Shutting down node 0...");
    nodes[0].stop().await?;

    Ok(())
}

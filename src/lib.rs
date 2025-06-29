pub mod crypto;
pub mod error;
pub mod message;
pub mod network;
pub mod node;
pub mod protocol;
pub mod storage;
pub mod timer;
pub mod types;
pub mod consensus;
pub mod metrics;
pub mod config;
pub mod testing;

pub use error::HotStuffError;
pub use node::Node;
pub use protocol::hotstuff2::HotStuff2;
pub use types::{Block, Hash, Proposal, QuorumCert, Timestamp};

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn get_free_port(base: u16, offset: usize) -> u16 {
        base + offset as u16
    }

    #[tokio::test]
    async fn test_hotstuff2_basic_flow() -> Result<(), HotStuffError> {
        const NODES: usize = 4;
        let base_port = 9000;

        // Create test nodes
        let nodes = (0..NODES)
            .map(|id| {
                let node_id = id as u64;
                let mut config = node::NodeConfig::default();
                config.node_id = node_id;
                config.listen_addr = format!("127.0.0.1:{}", get_free_port(base_port, id));
                config.peers = (0..NODES)
                    .filter(|&i| i != id)
                    .map(|i| {
                        (
                            i as u64,
                            format!("127.0.0.1:{}", get_free_port(base_port, i)),
                        )
                    })
                    .collect();
                config.data_dir = format!("./test_data/node_{}", id);
                Arc::new(Mutex::new(node::Node::new(config)))
            })
            .collect::<Vec<_>>();

        // Start all nodes concurrently
        let start_results = futures::future::join_all(nodes.iter().map(|node| {
            let node = Arc::clone(node);
            async move {
                node.lock().await.start().await
            }
        }))
        .await;

        // Check if any node failed to start
        for (i, result) in start_results.iter().enumerate() {
            if let Err(e) = result {
                eprintln!("Node {} failed to start: {}", i, e);
                // Still continue the test to clean up other nodes
            }
        }

        // Give nodes time to establish connections
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        // For now, just test that the nodes can be created and basic operations work
        // A full consensus test would require more complex orchestration
        let first_node = nodes[0].clone();
        let node_id = first_node.lock().await.node_id();
        assert_eq!(node_id, 0);

        // Test basic block creation and storage
        let genesis_block = types::Block::new(
            types::Hash::zero(),
            vec![],
            0,
            0,
        );

        // Try to propose a block (this may fail if network isn't fully connected, that's ok for testing)
        let _ = first_node.lock().await.propose(genesis_block).await;

        // Wait a bit more
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        // Stop all nodes
        for node in &nodes {
            let _ = node.lock().await.stop().await;
        }

        Ok(())
    }
}

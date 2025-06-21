pub mod crypto;
pub mod error;
pub mod message;
pub mod network;
pub mod node;
pub mod protocol;
pub mod storage;
pub mod timer;
pub mod types;

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
        const FAULTS: usize = 1;
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
                Arc::new(Mutex::new(Node::new(config)))
            })
            .collect::<Vec<_>>();

        // Start all nodes concurrently
        futures::future::join_all(nodes.iter().map(|node| {
            let node = Arc::clone(node);
            async move {
                node.lock()
                    .await
                    .start()
                    .await
                    .expect("Node failed to start");
            }
        }))
        .await;

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Leader proposes a block
        let leader = nodes[0].clone();
        let leader_id = leader.lock().await.node_id();
        let proposal = Proposal {
            block: Block::new(Hash::zero(), vec![], 1, leader_id),
            timestamp: Timestamp::now(),
        };
        leader.lock().await.propose(proposal.block.clone()).await?;

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Verify all non-faulty nodes committed the block
        let committed = futures::future::join_all(
            nodes
                .iter()
                .take(NODES - FAULTS)
                .map(|node| async { node.lock().await.committed_block().await.is_some() }),
        )
        .await
        .into_iter()
        .all(|x| x);

        assert!(committed, "Consensus not reached");

        // Stop all nodes
        for node in &nodes {
            node.lock().await.stop().await?;
        }

        Ok(())
    }
}

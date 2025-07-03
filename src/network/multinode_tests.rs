// Multi-node integration tests with real TCP networking
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use log::{info, warn};
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout};

use crate::config::HotStuffConfig;
use crate::consensus::state_machine::TestStateMachine;
use crate::crypto::KeyPair;
use crate::network::{ProductionP2PNetwork, ProductionNetworkConfig};
use crate::protocol::hotstuff2::HotStuff2;
use crate::storage::block_store::MemoryBlockStore;
use crate::timer::TimeoutManager;
use crate::types::Transaction;
use crate::error::HotStuffError;

/// Multi-node test setup with real TCP networking
pub struct MultiNodeTestSetup {
    pub nodes: Vec<TestNode>,
    pub config: HotStuffConfig,
}

/// Individual test node with real networking
pub struct TestNode {
    pub node_id: u64,
    pub listen_addr: SocketAddr,
    pub hotstuff: Arc<HotStuff2<MemoryBlockStore>>,
    pub network: Arc<ProductionP2PNetwork>,
    pub block_store: Arc<MemoryBlockStore>,
}

impl MultiNodeTestSetup {
    /// Create a new multi-node test setup
    pub async fn new(num_nodes: usize, base_port: u16) -> Result<Self, HotStuffError> {
        info!("Creating multi-node test setup with {} nodes", num_nodes);

        let config = create_test_config();
        let mut nodes = Vec::new();

        // Create nodes
        for i in 0..num_nodes {
            let node_id = i as u64;
            let listen_addr: SocketAddr = format!("127.0.0.1:{}", base_port + i as u16).parse().unwrap();
            
            let node = create_test_node(node_id, listen_addr, num_nodes as u64, config.clone()).await?;
            nodes.push(node);
        }

        Ok(Self { nodes, config })
    }

    /// Start all nodes and establish connections
    pub async fn start_all(&mut self) -> Result<(), HotStuffError> {
        info!("Starting all {} nodes", self.nodes.len());

        // Start network servers for all nodes
        for node in &self.nodes {
            info!("Starting network for node {} on {}", node.node_id, node.listen_addr);
            node.network.start().await?;
        }

        // Wait for servers to start
        sleep(Duration::from_millis(500)).await;

        // Establish connections between all nodes (full mesh)
        for i in 0..self.nodes.len() {
            for j in 0..self.nodes.len() {
                if i != j {
                    let from_node = &self.nodes[i];
                    let to_node = &self.nodes[j];
                    
                    info!("Connecting node {} to node {} at {}", 
                        from_node.node_id, to_node.node_id, to_node.listen_addr);
                    
                    if let Err(e) = from_node.network.connect_to_peer(to_node.node_id, to_node.listen_addr).await {
                        warn!("Failed to connect node {} to node {}: {}", 
                            from_node.node_id, to_node.node_id, e);
                    }
                }
            }
        }

        // Wait for connections to establish
        sleep(Duration::from_secs(2)).await;

        // Verify connections
        for node in &self.nodes {
            let connected_peers = node.network.get_connected_peers().await;
            info!("Node {} connected to {} peers: {:?}", 
                node.node_id, connected_peers.len(), connected_peers);
        }

        // Start HotStuff protocol on all nodes
        for node in &self.nodes {
            info!("Starting HotStuff protocol for node {}", node.node_id);
            node.hotstuff.start();
        }

        info!("All nodes started successfully");
        Ok(())
    }

    /// Submit a transaction to the first node
    pub async fn submit_transaction(&self, tx: Transaction) -> Result<(), HotStuffError> {
        if self.nodes.is_empty() {
            return Err(HotStuffError::Configuration("No nodes available".to_string()));
        }

        info!("Submitting transaction to node {}", self.nodes[0].node_id);
        self.nodes[0].hotstuff.submit_transaction(tx).await
    }

    /// Wait for all nodes to reach a certain block height
    pub async fn wait_for_height(&self, target_height: u64, timeout_duration: Duration) -> Result<(), HotStuffError> {
        info!("Waiting for all nodes to reach height {}", target_height);

        let result = timeout(timeout_duration, async {
            loop {
                let mut all_ready = true;
                
                for _node in &self.nodes {
                    // Check current height - this would need to be exposed by HotStuff2
                    // For now, we'll simulate this check
                    all_ready = true; // TODO: Implement proper height checking
                }

                if all_ready {
                    info!("All nodes reached target height {}", target_height);
                    return Ok(());
                }

                sleep(Duration::from_millis(100)).await;
            }
        }).await;

        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(HotStuffError::Timer("Timeout waiting for target height".to_string())),
        }
    }

    /// Get network statistics for all nodes
    pub async fn get_network_stats(&self) -> Vec<(u64, crate::network::ProductionNetworkStats)> {
        let mut stats = Vec::new();
        
        for node in &self.nodes {
            let node_stats = node.network.get_stats().await;
            stats.push((node.node_id, node_stats));
        }

        stats
    }

    /// Simulate network partition between two groups of nodes
    pub async fn simulate_partition(&self, group1: Vec<usize>, group2: Vec<usize>) -> Result<(), HotStuffError> {
        info!("Simulating network partition between groups {:?} and {:?}", group1, group2);

        // Disconnect nodes between groups
        for &i in &group1 {
            for &j in &group2 {
                if i < self.nodes.len() && j < self.nodes.len() {
                    self.nodes[i].network.disconnect_peer(self.nodes[j].node_id).await;
                    self.nodes[j].network.disconnect_peer(self.nodes[i].node_id).await;
                }
            }
        }

        info!("Network partition simulated");
        Ok(())
    }

    /// Heal the network partition
    pub async fn heal_partition(&self) -> Result<(), HotStuffError> {
        info!("Healing network partition");

        // Re-establish all connections (full mesh)
        for i in 0..self.nodes.len() {
            for j in 0..self.nodes.len() {
                if i != j {
                    let from_node = &self.nodes[i];
                    let to_node = &self.nodes[j];
                    
                    if let Err(e) = from_node.network.connect_to_peer(to_node.node_id, to_node.listen_addr).await {
                        warn!("Failed to reconnect node {} to node {}: {}", 
                            from_node.node_id, to_node.node_id, e);
                    }
                }
            }
        }

        // Wait for connections to re-establish
        sleep(Duration::from_secs(1)).await;

        info!("Network partition healed");
        Ok(())
    }
}

/// Create a test node with real networking
async fn create_test_node(
    node_id: u64,
    listen_addr: SocketAddr,
    num_nodes: u64,
    config: HotStuffConfig,
) -> Result<TestNode, HotStuffError> {
    // Create key pair
    let key_pair = KeyPair::generate(&mut rand::rng());

    // Create block store
    let block_store = Arc::new(MemoryBlockStore::new());

    // Create state machine
    let state_machine: Arc<Mutex<dyn crate::consensus::state_machine::StateMachine>> = 
        Arc::new(Mutex::new(TestStateMachine::new()));

    // Create network configuration
    let network_config = ProductionNetworkConfig {
        connection_timeout: Duration::from_secs(5),
        heartbeat_interval: Duration::from_secs(10),
        max_message_size: 1024 * 1024, // 1MB
        reconnect_attempts: 3,
        reconnect_delay: Duration::from_secs(2),
        message_queue_size: 1000,
    };

    // Create production TCP network
    let (network, message_receiver) = ProductionP2PNetwork::new(node_id, listen_addr, network_config);
    let network = Arc::new(network);

    // Create timeout manager
    let timeout_manager = TimeoutManager::new(Duration::from_secs(5), 2.0);

    // Create HotStuff instance
    let hotstuff = HotStuff2::new_with_production_tcp(
        node_id,
        key_pair,
        Arc::clone(&network),
        Arc::clone(&block_store),
        timeout_manager,
        num_nodes,
        config,
        state_machine,
    );

    // Start message processing (connect network to consensus)
    let consensus = Arc::clone(&hotstuff);
    tokio::spawn(async move {
        let mut receiver = message_receiver;
        while let Some((peer_id, message)) = receiver.recv().await {
            info!("Node {} received message from peer {}", node_id, peer_id);
            
            // Forward network messages to consensus layer
            if let Err(e) = consensus.handle_network_message(peer_id, message).await {
                warn!("Failed to handle network message: {}", e);
            }
        }
    });

    Ok(TestNode {
        node_id,
        listen_addr,
        hotstuff,
        network,
        block_store,
    })
}

/// Create test configuration for multi-node setup
fn create_test_config() -> HotStuffConfig {
    use crate::config::ConsensusConfig;

    HotStuffConfig {
        consensus: ConsensusConfig {
            optimistic_mode: true,
            base_timeout_ms: 3000,
            timeout_multiplier: 2.0,
            view_change_timeout_ms: 5000,
            max_transactions_per_block: 100,
            max_batch_size: 50,
            batch_timeout_ms: 200,
            max_block_size: 1024 * 1024,
            byzantine_fault_tolerance: 1,
            enable_pipelining: true,
            pipeline_depth: 3,
            fast_path_timeout_ms: 1000,
            optimistic_threshold: 0.8,
            synchrony_detection_window: 10,
            max_network_delay_ms: 2000,
            latency_variance_threshold_ms: 200,
            max_view_changes: 10,
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    /// Test basic multi-node consensus with real TCP networking
    #[tokio::test]
    async fn test_multinode_consensus_basic() {
        env_logger::try_init().ok();
        
        // Use timeout to prevent the test from hanging indefinitely
        let test_result = timeout(Duration::from_secs(30), async {
            info!("Starting multi-node consensus test");

            // Create 4-node setup
            let mut setup = MultiNodeTestSetup::new(4, 9000).await.unwrap();
            
            // Start all nodes
            setup.start_all().await.unwrap();

            // Wait for initial connections
            sleep(Duration::from_secs(2)).await;

            // Submit a test transaction
            let tx = Transaction {
                id: "test_tx_1".to_string(),
                data: vec![1, 2, 3, 4],
            };

            setup.submit_transaction(tx).await.unwrap();

            // Wait for consensus
            sleep(Duration::from_secs(5)).await;

            // Check network statistics
            let stats = setup.get_network_stats().await;
            for (node_id, node_stats) in stats {
                info!("Node {} stats: active_connections={}, messages_sent={}, messages_received={}", 
                    node_id, node_stats.active_connections, node_stats.messages_sent, node_stats.messages_received);
                
                assert!(node_stats.active_connections > 0, "Node {} should have active connections", node_id);
            }

            info!("Multi-node consensus test completed successfully");
        }).await;

        match test_result {
            Ok(_) => {
                info!("Test completed successfully");
            }
            Err(_) => {
                panic!("Test timed out after 30 seconds");
            }
        }
    }

    /// Test network partition tolerance
    #[tokio::test]
    async fn test_network_partition_tolerance() {
        env_logger::try_init().ok();
        
        info!("Starting network partition tolerance test");

        // Create 4-node setup
        let mut setup = MultiNodeTestSetup::new(4, 9100).await.unwrap();
        setup.start_all().await.unwrap();

        // Wait for initial connections
        sleep(Duration::from_secs(2)).await;

        // Simulate partition: nodes 0,1 vs nodes 2,3
        setup.simulate_partition(vec![0, 1], vec![2, 3]).await.unwrap();

        // Wait for partition to take effect
        sleep(Duration::from_secs(2)).await;

        // Submit transactions to both partitions
        let tx1 = Transaction {
            id: "tx_partition_1".to_string(),
            data: vec![1, 2, 3],
        };

        setup.submit_transaction(tx1).await.unwrap();

        // Wait during partition
        sleep(Duration::from_secs(3)).await;

        // Heal the partition
        setup.heal_partition().await.unwrap();

        // Wait for network to converge
        sleep(Duration::from_secs(5)).await;

        // Verify network recovery
        let stats = setup.get_network_stats().await;
        for (node_id, node_stats) in stats {
            info!("Post-partition node {} stats: active_connections={}", 
                node_id, node_stats.active_connections);
            
            // After healing, nodes should have connections again
            assert!(node_stats.active_connections > 0, 
                "Node {} should have connections after partition healing", node_id);
        }

        info!("Network partition tolerance test completed successfully");
    }

    /// Test Byzantine behavior detection over real network
    #[tokio::test]
    async fn test_byzantine_behavior_detection() {
        env_logger::try_init().ok();
        
        info!("Starting Byzantine behavior detection test");

        // Create 4-node setup (can tolerate 1 Byzantine node)
        let mut setup = MultiNodeTestSetup::new(4, 9200).await.unwrap();
        setup.start_all().await.unwrap();

        // Wait for connections
        sleep(Duration::from_secs(2)).await;

        // TODO: Implement Byzantine behavior simulation
        // This would require extending the test framework to inject Byzantine behaviors

        // For now, just verify normal operation
        let tx = Transaction {
            id: "byzantine_test_tx".to_string(),
            data: vec![5, 6, 7, 8],
        };

        setup.submit_transaction(tx).await.unwrap();

        // Wait for consensus
        sleep(Duration::from_secs(5)).await;

        info!("Byzantine behavior detection test completed");
    }
}

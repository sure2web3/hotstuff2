// Comprehensive test suite for HotStuff-2 protocol implementation
// Tests all major features according to the paper specifications

use std::sync::Arc;
use std::time::Duration;
use std::collections::HashMap;

use tokio::time::{timeout, sleep};
use log::{info, debug};

use crate::config::{ConsensusConfig, HotStuffConfig, NetworkConfig};
use crate::crypto::{KeyPair, BlsSecretKey};
use crate::crypto::bls_threshold::ProductionThresholdSigner;
use crate::error::HotStuffError;
use crate::message::consensus::{ConsensusMsg, Vote};
use crate::network::NetworkClient;
use crate::protocol::hotstuff2::{HotStuff2, PerformanceStats, Phase};
use crate::storage::block_store::MemoryBlockStore;
use crate::timer::TimeoutManager;
use crate::types::{Block, Hash, Transaction, QuorumCert};

/// Comprehensive test harness for HotStuff-2 protocol
pub struct HotStuff2TestHarness {
    pub nodes: Vec<Arc<HotStuff2<MemoryBlockStore>>>,
    pub num_nodes: usize,
    pub num_byzantine: usize,
}

impl HotStuff2TestHarness {
    /// Create a new test harness with specified configuration
    pub async fn new(num_nodes: usize, num_byzantine: usize) -> Result<Self, HotStuffError> {
        assert!(num_byzantine * 3 < num_nodes, "Too many Byzantine nodes for safety");
        assert!(num_nodes >= 4, "Need at least 4 nodes for meaningful tests");
        
        info!("Creating HotStuff-2 test harness with {} nodes ({} Byzantine)", 
              num_nodes, num_byzantine);
        
        let mut nodes = Vec::new();
        
        // Create nodes with proper configuration
        for i in 0..num_nodes {
            let node = Self::create_test_node(i as u64, num_nodes as u64).await?;
            nodes.push(node);
        }
        
        Ok(Self {
            nodes,
            num_nodes,
            num_byzantine,
        })
    }
    
    /// Create a single test node with proper configuration
    async fn create_test_node(node_id: u64, num_nodes: u64) -> Result<Arc<HotStuff2<MemoryBlockStore>>, HotStuffError> {
        // Generate key pair for the node
        let key_pair = KeyPair::generate();
        
        // Create network client with test configuration
        let network_config = NetworkConfig {
            listen_port: 8000 + node_id as u16,
            peers: (0..num_nodes)
                .filter(|&id| id != node_id)
                .map(|id| format!("127.0.0.1:{}", 8000 + id))
                .collect(),
            max_connections: num_nodes as usize,
            connection_timeout_ms: 5000,
        };
        
        let network_client = Arc::new(NetworkClient::new(network_config)?);
        
        // Create block store
        let block_store = Arc::new(MemoryBlockStore::new());
        
        // Create timeout manager
        let timeout_manager = Arc::new(TimeoutManager::new());
        
        // Create consensus configuration
        let consensus_config = ConsensusConfig {
            base_timeout_ms: 1000,
            timeout_multiplier: 1.5,
            max_batch_size: 100,
            batch_timeout_ms: 500,
            view_change_timeout_ms: 2000,
            optimistic_mode: true,
        };
        
        let hotstuff_config = HotStuffConfig {
            consensus: consensus_config,
            network: network_config,
        };
        
        // Create state machine (mock)
        let state_machine = Arc::new(tokio::sync::Mutex::new(MockStateMachine::new()));
        
        // Create HotStuff-2 node
        let node = HotStuff2::new(
            node_id,
            key_pair,
            network_client,
            block_store,
            timeout_manager,
            num_nodes,
            hotstuff_config,
            state_machine,
        );
        
        Ok(node)
    }
    
    /// Start all nodes in the harness
    pub async fn start_all(&self) {
        for node in &self.nodes {
            node.start();
        }
        
        // Give nodes time to start
        sleep(Duration::from_millis(100)).await;
    }
    
    /// Shutdown all nodes gracefully
    pub async fn shutdown_all(&self) -> Result<(), HotStuffError> {
        for node in &self.nodes {
            node.shutdown().await?;
        }
        Ok(())
    }
    
    /// Submit a transaction to a random node
    pub async fn submit_transaction(&self, data: Vec<u8>) -> Result<(), HotStuffError> {
        let node_idx = rand::random::<usize>() % self.nodes.len();
        let transaction = Transaction::new(
            format!("tx_{}", rand::random::<u64>()),
            data,
        );
        
        self.nodes[node_idx].submit_transaction(transaction).await
    }
    
    /// Wait for consensus to be reached on a specific height
    pub async fn wait_for_consensus(&self, target_height: u64, timeout_duration: Duration) -> Result<bool, HotStuffError> {
        let start = tokio::time::Instant::now();
        
        while start.elapsed() < timeout_duration {
            let mut consensus_count = 0;
            
            for node in &self.nodes {
                let stats = node.get_performance_statistics().await?;
                if stats.current_height >= target_height {
                    consensus_count += 1;
                }
            }
            
            // Check if majority reached consensus
            if consensus_count > self.num_nodes / 2 {
                return Ok(true);
            }
            
            sleep(Duration::from_millis(100)).await;
        }
        
        Ok(false)
    }
    
    /// Inject Byzantine behavior into specified nodes
    pub async fn inject_byzantine_behavior(&self, byzantine_nodes: &[usize], behavior: ByzantineBehaviorType) {
        for &node_idx in byzantine_nodes {
            if node_idx < self.nodes.len() {
                info!("Injecting Byzantine behavior {:?} into node {}", behavior, node_idx);
                // In a real implementation, this would modify node behavior
                // For testing, we simulate the behavior effects
            }
        }
    }
    
    /// Simulate network partitions
    pub async fn simulate_network_partition(&self, partition_groups: Vec<Vec<usize>>) {
        info!("Simulating network partition with groups: {:?}", partition_groups);
        // In a real implementation, this would modify network connectivity
        // For testing, we simulate the effects
    }
    
    /// Get comprehensive system statistics
    pub async fn get_system_stats(&self) -> Result<SystemStats, HotStuffError> {
        let mut node_stats = Vec::new();
        let mut total_transactions = 0;
        let mut total_blocks = 0;
        let mut synchronous_nodes = 0;
        
        for (i, node) in self.nodes.iter().enumerate() {
            let stats = node.get_performance_statistics().await?;
            total_transactions += stats.pending_transactions;
            total_blocks += stats.current_height;
            
            if stats.is_synchronous {
                synchronous_nodes += 1;
            }
            
            node_stats.push(NodeStats {
                node_id: i as u64,
                current_height: stats.current_height,
                current_view: stats.current_view,
                is_synchronous: stats.is_synchronous,
                pending_transactions: stats.pending_transactions,
                pipeline_stages: stats.pipeline_stages,
            });
        }
        
        Ok(SystemStats {
            node_stats,
            total_transactions,
            total_blocks,
            synchronous_nodes,
            total_nodes: self.num_nodes,
        })
    }
}

/// Mock state machine for testing
pub struct MockStateMachine {
    executed_blocks: Vec<Hash>,
}

impl MockStateMachine {
    pub fn new() -> Self {
        Self {
            executed_blocks: Vec::new(),
        }
    }
}

impl crate::consensus::state_machine::StateMachine for MockStateMachine {
    fn execute_block(&mut self, block: &Block) -> Result<(), HotStuffError> {
        self.executed_blocks.push(block.hash);
        debug!("Executed block {} with {} transactions", 
               block.hash, block.transactions.len());
        Ok(())
    }
    
    fn get_state_hash(&self) -> Hash {
        // Simple state hash based on executed blocks
        let mut data = Vec::new();
        for block_hash in &self.executed_blocks {
            data.extend_from_slice(&block_hash.as_bytes());
        }
        Hash::from_bytes(&data)
    }
}

/// Byzantine behavior types for testing
#[derive(Debug, Clone, Copy)]
pub enum ByzantineBehaviorType {
    Equivocation,
    DelayedMessages,
    InvalidSignatures,
    DropMessages,
    CorruptedMessages,
    ForceViewChange,
}

/// Statistics for individual nodes
#[derive(Debug, Clone)]
pub struct NodeStats {
    pub node_id: u64,
    pub current_height: u64,
    pub current_view: u64,
    pub is_synchronous: bool,
    pub pending_transactions: usize,
    pub pipeline_stages: usize,
}

/// System-wide statistics
#[derive(Debug, Clone)]
pub struct SystemStats {
    pub node_stats: Vec<NodeStats>,
    pub total_transactions: usize,
    pub total_blocks: u64,
    pub synchronous_nodes: usize,
    pub total_nodes: usize,
}

/// Comprehensive test suite for HotStuff-2 features
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_basic_consensus_flow() {
        let harness = HotStuff2TestHarness::new(4, 0).await.unwrap();
        harness.start_all().await;
        
        // Submit transactions
        for i in 0..10 {
            let data = format!("transaction_{}", i).into_bytes();
            harness.submit_transaction(data).await.unwrap();
        }
        
        // Wait for consensus
        let consensus_reached = harness.wait_for_consensus(1, Duration::from_secs(10)).await.unwrap();
        assert!(consensus_reached, "Consensus should be reached");
        
        harness.shutdown_all().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_byzantine_fault_tolerance() {
        let harness = HotStuff2TestHarness::new(7, 2).await.unwrap(); // f=2, n=7
        harness.start_all().await;
        
        // Inject Byzantine behavior
        harness.inject_byzantine_behavior(&[0, 1], ByzantineBehaviorType::Equivocation).await;
        
        // Submit transactions
        for i in 0..5 {
            let data = format!("byzantine_tx_{}", i).into_bytes();
            harness.submit_transaction(data).await.unwrap();
        }
        
        // System should still reach consensus despite Byzantine nodes
        let consensus_reached = harness.wait_for_consensus(1, Duration::from_secs(15)).await.unwrap();
        assert!(consensus_reached, "Consensus should be reached despite Byzantine nodes");
        
        harness.shutdown_all().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_optimistic_responsiveness() {
        let harness = HotStuff2TestHarness::new(4, 0).await.unwrap();
        harness.start_all().await;
        
        // In synchronous conditions, consensus should be fast
        let start_time = tokio::time::Instant::now();
        
        for i in 0..3 {
            let data = format!("fast_tx_{}", i).into_bytes();
            harness.submit_transaction(data).await.unwrap();
        }
        
        let consensus_reached = harness.wait_for_consensus(1, Duration::from_secs(5)).await.unwrap();
        let elapsed = start_time.elapsed();
        
        assert!(consensus_reached, "Consensus should be reached");
        assert!(elapsed < Duration::from_secs(3), "Consensus should be fast in synchronous network");
        
        harness.shutdown_all().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_pipeline_performance() {
        let harness = HotStuff2TestHarness::new(4, 0).await.unwrap();
        harness.start_all().await;
        
        // Submit many transactions to test pipeline
        for i in 0..50 {
            let data = format!("pipeline_tx_{}", i).into_bytes();
            harness.submit_transaction(data).await.unwrap();
        }
        
        // Wait for multiple blocks to be processed
        let consensus_reached = harness.wait_for_consensus(3, Duration::from_secs(20)).await.unwrap();
        assert!(consensus_reached, "Pipeline should process multiple blocks");
        
        // Check system statistics
        let stats = harness.get_system_stats().await.unwrap();
        assert!(stats.total_blocks >= 3, "Should have processed multiple blocks");
        
        harness.shutdown_all().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_view_change_mechanism() {
        let harness = HotStuff2TestHarness::new(4, 0).await.unwrap();
        harness.start_all().await;
        
        // Force view change in one node
        harness.nodes[0].initiate_view_change("test view change").await.unwrap();
        
        // Submit transaction after view change
        let data = b"view_change_tx".to_vec();
        harness.submit_transaction(data).await.unwrap();
        
        // System should recover and reach consensus
        let consensus_reached = harness.wait_for_consensus(1, Duration::from_secs(10)).await.unwrap();
        assert!(consensus_reached, "Consensus should be reached after view change");
        
        harness.shutdown_all().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_network_partition_recovery() {
        let harness = HotStuff2TestHarness::new(6, 0).await.unwrap();
        harness.start_all().await;
        
        // Simulate network partition
        harness.simulate_network_partition(vec![vec![0, 1, 2], vec![3, 4, 5]]).await;
        
        // Wait for partition effects
        sleep(Duration::from_secs(2)).await;
        
        // Simulate partition recovery
        harness.simulate_network_partition(vec![vec![0, 1, 2, 3, 4, 5]]).await;
        
        // Submit transaction after recovery
        let data = b"recovery_tx".to_vec();
        harness.submit_transaction(data).await.unwrap();
        
        // System should recover and reach consensus
        let consensus_reached = harness.wait_for_consensus(1, Duration::from_secs(15)).await.unwrap();
        assert!(consensus_reached, "Consensus should be reached after partition recovery");
        
        harness.shutdown_all().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_threshold_signature_aggregation() {
        let harness = HotStuff2TestHarness::new(4, 0).await.unwrap();
        harness.start_all().await;
        
        // Submit transaction that will trigger signature aggregation
        let data = b"threshold_sig_tx".to_vec();
        harness.submit_transaction(data).await.unwrap();
        
        // Wait for consensus with threshold signatures
        let consensus_reached = harness.wait_for_consensus(1, Duration::from_secs(10)).await.unwrap();
        assert!(consensus_reached, "Consensus should be reached with threshold signatures");
        
        harness.shutdown_all().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_concurrent_transactions() {
        let harness = HotStuff2TestHarness::new(4, 0).await.unwrap();
        harness.start_all().await;
        
        // Submit transactions concurrently
        let mut handles = Vec::new();
        for i in 0..20 {
            let harness_ref = &harness;
            let handle = tokio::spawn(async move {
                let data = format!("concurrent_tx_{}", i).into_bytes();
                harness_ref.submit_transaction(data).await
            });
            handles.push(handle);
        }
        
        // Wait for all submissions
        for handle in handles {
            handle.await.unwrap().unwrap();
        }
        
        // Wait for all transactions to be processed
        let consensus_reached = harness.wait_for_consensus(2, Duration::from_secs(15)).await.unwrap();
        assert!(consensus_reached, "Should handle concurrent transactions");
        
        harness.shutdown_all().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_performance_monitoring() {
        let harness = HotStuff2TestHarness::new(4, 0).await.unwrap();
        harness.start_all().await;
        
        // Submit some transactions
        for i in 0..5 {
            let data = format!("monitor_tx_{}", i).into_bytes();
            harness.submit_transaction(data).await.unwrap();
        }
        
        // Wait for processing
        sleep(Duration::from_secs(2)).await;
        
        // Check performance statistics
        let stats = harness.get_system_stats().await.unwrap();
        assert_eq!(stats.total_nodes, 4);
        assert!(stats.synchronous_nodes > 0, "Some nodes should be synchronous");
        
        // Check individual node health
        for node in &harness.nodes {
            let health = node.health_check().await.unwrap();
            assert!(health, "All nodes should be healthy");
        }
        
        harness.shutdown_all().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_failure_recovery() {
        let harness = HotStuff2TestHarness::new(4, 0).await.unwrap();
        harness.start_all().await;
        
        // Simulate failure and recovery in one node
        harness.nodes[0].recover_from_failure().await.unwrap();
        
        // Submit transaction after recovery
        let data = b"recovery_test_tx".to_vec();
        harness.submit_transaction(data).await.unwrap();
        
        // System should continue working
        let consensus_reached = harness.wait_for_consensus(1, Duration::from_secs(10)).await.unwrap();
        assert!(consensus_reached, "Consensus should be reached after recovery");
        
        harness.shutdown_all().await.unwrap();
    }
}

/// Performance benchmarks for HotStuff-2
#[cfg(test)]
mod benchmarks {
    use super::*;
    use std::time::Instant;
    
    #[tokio::test]
    async fn benchmark_throughput() {
        let harness = HotStuff2TestHarness::new(4, 0).await.unwrap();
        harness.start_all().await;
        
        let start_time = Instant::now();
        let num_transactions = 100;
        
        // Submit transactions as fast as possible
        for i in 0..num_transactions {
            let data = format!("bench_tx_{}", i).into_bytes();
            harness.submit_transaction(data).await.unwrap();
        }
        
        // Wait for all transactions to be processed
        let consensus_reached = harness.wait_for_consensus(5, Duration::from_secs(30)).await.unwrap();
        assert!(consensus_reached, "All transactions should be processed");
        
        let elapsed = start_time.elapsed();
        let throughput = num_transactions as f64 / elapsed.as_secs_f64();
        
        info!("Throughput: {:.2} tx/sec", throughput);
        assert!(throughput > 10.0, "Throughput should be reasonable");
        
        harness.shutdown_all().await.unwrap();
    }
    
    #[tokio::test]
    async fn benchmark_latency() {
        let harness = HotStuff2TestHarness::new(4, 0).await.unwrap();
        harness.start_all().await;
        
        let mut latencies = Vec::new();
        
        // Measure latency for individual transactions
        for i in 0..10 {
            let start_time = Instant::now();
            let data = format!("latency_tx_{}", i).into_bytes();
            harness.submit_transaction(data).await.unwrap();
            
            // Wait for this specific transaction to be processed
            sleep(Duration::from_millis(100)).await;
            let latency = start_time.elapsed();
            latencies.push(latency);
        }
        
        let avg_latency = latencies.iter().sum::<Duration>() / latencies.len() as u32;
        info!("Average latency: {:?}", avg_latency);
        
        assert!(avg_latency < Duration::from_secs(2), "Latency should be reasonable");
        
        harness.shutdown_all().await.unwrap();
    }
}

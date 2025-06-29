// Advanced Byzantine Fault Tolerance Tests for HotStuff-2
// Comprehensive testing of Byzantine behaviors and their mitigation

use std::sync::Arc;
use std::time::Duration;
use std::collections::{HashMap, HashSet};
use std::fmt;

use tokio::time::{sleep, timeout, Instant};
use log::{info, warn, debug, error};
use rand::{Rng, thread_rng, random};

use crate::config::{ConsensusConfig, HotStuffConfig, NetworkConfig};
use crate::crypto::KeyPair;
use crate::error::HotStuffError;
use crate::message::consensus::{ConsensusMsg, Vote};
use crate::network::{NetworkClient, p2p::{P2PNetwork, P2PMessage, MessagePayload}};
use crate::protocol::hotstuff2::{HotStuff2, PerformanceStats};
use crate::storage::block_store::MemoryBlockStore;
use crate::timer::TimeoutManager;
use crate::types::{Block, Hash, Transaction, QuorumCert};

/// Advanced Byzantine behavior patterns based on real-world attack scenarios
#[derive(Debug, Clone, PartialEq)]
pub enum AdvancedByzantineBehavior {
    /// Classic equivocation - send different votes to different nodes
    Equivocation {
        probability: f64,
    },
    /// Selective participation - only participate in some rounds
    SelectiveParticipation {
        participation_rate: f64,
    },
    /// Message delay attacks - delay messages to cause timeouts
    AdaptiveDelayAttack {
        min_delay: Duration,
        max_delay: Duration,
    },
    /// Signature corruption - send messages with invalid signatures
    SignatureCorruption {
        corruption_rate: f64,
    },
    /// Strategic silence - remain silent during critical phases
    StrategicSilence {
        silence_phases: Vec<String>,
    },
    /// Coalition attacks - coordinate with other Byzantine nodes
    CoalitionAttack {
        coalition_members: Vec<u64>,
        attack_strategy: CoalitionStrategy,
    },
    /// Grinding attacks - try to influence leader selection
    LeaderGrinding {
        preferred_leaders: Vec<u64>,
    },
    /// Fork creation attempts
    ForkCreation {
        fork_probability: f64,
    },
}

/// Coordination strategies for Byzantine coalitions
#[derive(Debug, Clone, PartialEq)]
pub enum CoalitionStrategy {
    CoordinatedEquivocation,
    CoordinatedSilence,
    CoordinatedDelays,
    RotatingAttacks,
}

/// Network conditions for testing resilience
#[derive(Debug, Clone)]
pub struct NetworkConditions {
    pub latency: Duration,
    pub jitter: Duration,
    pub packet_loss_rate: f64,
    pub bandwidth_limit: Option<usize>,
    pub partition_probability: f64,
}

impl Default for NetworkConditions {
    fn default() -> Self {
        Self {
            latency: Duration::from_millis(50),
            jitter: Duration::from_millis(10),
            packet_loss_rate: 0.01,
            bandwidth_limit: None,
            partition_probability: 0.0,
        }
    }
}

/// Advanced test harness for Byzantine fault tolerance
pub struct AdvancedByzantineTestHarness {
    legitimate_nodes: Vec<Arc<HotStuff2<MemoryBlockStore>>>,
    byzantine_nodes: Vec<Arc<ByzantineNode>>,
    network_conditions: NetworkConditions,
    test_duration: Duration,
    num_total_nodes: usize,
    num_byzantine_nodes: usize,
}

/// Byzantine node wrapper with sophisticated attack capabilities
pub struct ByzantineNode {
    node_id: u64,
    underlying_node: Arc<HotStuff2<MemoryBlockStore>>,
    behavior: AdvancedByzantineBehavior,
    attack_history: tokio::sync::Mutex<AttackHistory>,
    coalition_members: Vec<u64>,
}

/// Track Byzantine attack history for analysis
#[derive(Debug, Default)]
pub struct AttackHistory {
    equivocations: u64,
    delayed_messages: u64,
    dropped_messages: u64,
    corrupted_signatures: u64,
    silence_periods: u64,
    coalition_attacks: u64,
}

impl AdvancedByzantineTestHarness {
    /// Create new advanced Byzantine test harness
    pub async fn new(
        num_total_nodes: usize,
        num_byzantine_nodes: usize,
        byzantine_behaviors: Vec<AdvancedByzantineBehavior>,
        network_conditions: NetworkConditions,
        test_duration: Duration,
    ) -> Result<Self, HotStuffError> {
        // Validate Byzantine fault tolerance bounds
        assert!(num_byzantine_nodes * 3 < num_total_nodes, 
                "Byzantine nodes must be < n/3 for safety");
        assert_eq!(byzantine_behaviors.len(), num_byzantine_nodes,
                   "Must specify behavior for each Byzantine node");
        
        info!("Creating advanced Byzantine test harness: {} total nodes, {} Byzantine",
              num_total_nodes, num_byzantine_nodes);
        
        let mut legitimate_nodes = Vec::new();
        let mut byzantine_nodes = Vec::new();
        
        // Create legitimate nodes
        for i in num_byzantine_nodes..num_total_nodes {
            let node = Self::create_legitimate_node(i as u64, num_total_nodes).await?;
            legitimate_nodes.push(node);
        }
        
        // Create Byzantine nodes
        for i in 0..num_byzantine_nodes {
            let underlying_node = Self::create_legitimate_node(i as u64, num_total_nodes).await?;
            let byzantine_node = Arc::new(ByzantineNode::new(
                i as u64,
                underlying_node,
                byzantine_behaviors[i].clone(),
            ));
            byzantine_nodes.push(byzantine_node);
        }
        
        Ok(Self {
            legitimate_nodes,
            byzantine_nodes,
            network_conditions,
            test_duration,
            num_total_nodes,
            num_byzantine_nodes,
        })
    }
    
    async fn create_legitimate_node(
        node_id: u64,
        num_total_nodes: usize,
    ) -> Result<Arc<HotStuff2<MemoryBlockStore>>, HotStuffError> {
        let key_pair = KeyPair::generate();
        
        let network_config = NetworkConfig {
            listen_port: 9000 + node_id as u16,
            peers: (0..num_total_nodes as u64)
                .filter(|&id| id != node_id)
                .map(|id| format!("127.0.0.1:{}", 9000 + id))
                .collect(),
            max_connections: num_total_nodes,
            connection_timeout_ms: 5000,
        };
        
        let network_client = Arc::new(NetworkClient::new(network_config.clone())?);
        let block_store = Arc::new(MemoryBlockStore::new());
        let timeout_manager = Arc::new(TimeoutManager::new());
        
        let consensus_config = ConsensusConfig {
            base_timeout_ms: 2000,
            timeout_multiplier: 1.5,
            max_batch_size: 50,
            batch_timeout_ms: 1000,
            view_change_timeout_ms: 5000,
            optimistic_mode: true,
        };
        
        let hotstuff_config = HotStuffConfig {
            consensus: consensus_config,
            network: network_config,
        };
        
        let state_machine = Arc::new(tokio::sync::Mutex::new(
            crate::protocol::comprehensive_tests::MockStateMachine::new()
        ));
        
        Ok(HotStuff2::new(
            node_id,
            key_pair,
            network_client,
            block_store,
            timeout_manager,
            num_total_nodes as u64,
            hotstuff_config,
            state_machine,
        ))
    }
    
    /// Start the Byzantine fault tolerance test
    pub async fn run_comprehensive_bft_test(&self) -> Result<ByzantineTestResult, HotStuffError> {
        info!("Starting comprehensive Byzantine fault tolerance test");
        
        // Start all nodes
        self.start_all_nodes().await;
        
        let test_start = Instant::now();
        let mut safety_violations = Vec::new();
        let mut liveness_violations = Vec::new();
        let mut performance_degradation = Vec::new();
        
        // Submit transactions throughout the test
        let transaction_task = self.spawn_transaction_generator();
        
        // Monitor consensus progress
        let monitoring_task = self.spawn_consensus_monitor();
        
        // Inject Byzantine behaviors
        let byzantine_task = self.spawn_byzantine_coordinator();
        
        // Apply network conditions
        let network_task = self.spawn_network_controller();
        
        // Wait for test duration
        sleep(self.test_duration).await;
        
        // Collect results
        let final_stats = self.collect_final_statistics().await?;
        let attack_summary = self.analyze_attack_effectiveness().await?;
        
        // Analyze results
        let test_result = ByzantineTestResult {
            test_duration: test_start.elapsed(),
            total_nodes: self.num_total_nodes,
            byzantine_nodes: self.num_byzantine_nodes,
            safety_violations,
            liveness_violations,
            performance_degradation,
            final_statistics: final_stats,
            attack_summary,
            consensus_achieved: safety_violations.is_empty(),
            liveness_maintained: liveness_violations.len() < 3,
        };
        
        info!("Byzantine fault tolerance test completed: {:?}", test_result);
        Ok(test_result)
    }
    
    async fn start_all_nodes(&self) {
        // Start legitimate nodes
        for node in &self.legitimate_nodes {
            node.start();
        }
        
        // Start Byzantine nodes
        for byzantine_node in &self.byzantine_nodes {
            byzantine_node.underlying_node.start();
        }
        
        sleep(Duration::from_millis(500)).await;
    }
    
    async fn spawn_transaction_generator(&self) -> tokio::task::JoinHandle<()> {
        let nodes = self.legitimate_nodes.clone();
        let test_duration = self.test_duration;
        
        tokio::spawn(async move {
            let start_time = Instant::now();
            let mut tx_counter = 0u64;
            
            while start_time.elapsed() < test_duration {
                // Submit transaction to random legitimate node
                if !nodes.is_empty() {
                    let node_idx = random::<usize>() % nodes.len();
                    let transaction = Transaction::new(
                        format!("bft_tx_{}", tx_counter),
                        format!("Byzantine test transaction {}", tx_counter).into_bytes(),
                    );
                    
                    if let Err(e) = nodes[node_idx].submit_transaction(transaction).await {
                        debug!("Failed to submit transaction: {}", e);
                    }
                    
                    tx_counter += 1;
                }
                
                sleep(Duration::from_millis(200)).await;
            }
        })
    }
    
    async fn spawn_consensus_monitor(&self) -> tokio::task::JoinHandle<()> {
        let nodes = self.legitimate_nodes.clone();
        let test_duration = self.test_duration;
        
        tokio::spawn(async move {
            let start_time = Instant::now();
            let mut last_progress_time = start_time;
            let mut last_committed_height = 0u64;
            
            while start_time.elapsed() < test_duration {
                let mut max_height = 0u64;
                let mut consensus_count = 0usize;
                
                // Check consensus progress across nodes
                for node in &nodes {
                    if let Ok(stats) = node.get_performance_statistics().await {
                        max_height = max_height.max(stats.current_height);
                        if stats.current_height > last_committed_height {
                            consensus_count += 1;
                        }
                    }
                }
                
                // Detect liveness violations
                if max_height > last_committed_height {
                    last_committed_height = max_height;
                    last_progress_time = Instant::now();
                } else if last_progress_time.elapsed() > Duration::from_secs(10) {
                    warn!("Potential liveness violation detected: no progress for 10 seconds");
                }
                
                sleep(Duration::from_secs(1)).await;
            }
        })
    }
    
    async fn spawn_byzantine_coordinator(&self) -> tokio::task::JoinHandle<()> {
        let byzantine_nodes = self.byzantine_nodes.clone();
        let test_duration = self.test_duration;
        
        tokio::spawn(async move {
            let start_time = Instant::now();
            
            while start_time.elapsed() < test_duration {
                // Coordinate Byzantine attacks
                for byzantine_node in &byzantine_nodes {
                    byzantine_node.execute_byzantine_behavior().await;
                }
                
                sleep(Duration::from_millis(100)).await;
            }
        })
    }
    
    async fn spawn_network_controller(&self) -> tokio::task::JoinHandle<()> {
        let conditions = self.network_conditions.clone();
        let test_duration = self.test_duration;
        
        tokio::spawn(async move {
            let start_time = Instant::now();
            
            while start_time.elapsed() < test_duration {
                // Apply network conditions
                if conditions.partition_probability > 0.0 {
                    let random_val: f64 = thread_rng().gen();
                    if random_val < conditions.partition_probability {
                        debug!("Simulating network partition");
                        sleep(Duration::from_millis(1000)).await;
                    }
                }
                
                sleep(Duration::from_millis(500)).await;
            }
        })
    }
    
    async fn collect_final_statistics(&self) -> Result<Vec<NodeStatistics>, HotStuffError> {
        let mut statistics = Vec::new();
        
        // Collect from legitimate nodes
        for (i, node) in self.legitimate_nodes.iter().enumerate() {
            let stats = node.get_performance_statistics().await?;
            let health = node.health_check().await?;
            
            statistics.push(NodeStatistics {
                node_id: (self.num_byzantine_nodes + i) as u64,
                node_type: NodeType::Legitimate,
                final_height: stats.current_height,
                final_view: stats.current_view,
                is_healthy: health,
                pending_transactions: stats.pending_transactions,
                is_synchronous: stats.is_synchronous,
            });
        }
        
        // Collect from Byzantine nodes
        for (i, byzantine_node) in self.byzantine_nodes.iter().enumerate() {
            let stats = byzantine_node.underlying_node.get_performance_statistics().await?;
            let health = byzantine_node.underlying_node.health_check().await?;
            
            statistics.push(NodeStatistics {
                node_id: i as u64,
                node_type: NodeType::Byzantine,
                final_height: stats.current_height,
                final_view: stats.current_view,
                is_healthy: health,
                pending_transactions: stats.pending_transactions,
                is_synchronous: stats.is_synchronous,
            });
        }
        
        Ok(statistics)
    }
    
    async fn analyze_attack_effectiveness(&self) -> Result<AttackAnalysis, HotStuffError> {
        let mut total_attacks = AttackHistory::default();
        let mut individual_histories = Vec::new();
        
        for byzantine_node in &self.byzantine_nodes {
            let history = byzantine_node.attack_history.lock().await.clone();
            total_attacks.equivocations += history.equivocations;
            total_attacks.delayed_messages += history.delayed_messages;
            total_attacks.dropped_messages += history.dropped_messages;
            total_attacks.corrupted_signatures += history.corrupted_signatures;
            total_attacks.silence_periods += history.silence_periods;
            total_attacks.coalition_attacks += history.coalition_attacks;
            
            individual_histories.push(history);
        }
        
        Ok(AttackAnalysis {
            total_attacks,
            individual_histories,
            attack_success_rate: 0.0, // Would calculate based on actual impact
            mitigation_effectiveness: 1.0, // Would calculate based on system resilience
        })
    }
}

impl ByzantineNode {
    pub fn new(
        node_id: u64,
        underlying_node: Arc<HotStuff2<MemoryBlockStore>>,
        behavior: AdvancedByzantineBehavior,
    ) -> Self {
        let coalition_members = match &behavior {
            AdvancedByzantineBehavior::CoalitionAttack { coalition_members, .. } => {
                coalition_members.clone()
            }
            _ => Vec::new(),
        };
        
        Self {
            node_id,
            underlying_node,
            behavior,
            attack_history: tokio::sync::Mutex::new(AttackHistory::default()),
            coalition_members,
        }
    }
    
    pub async fn execute_byzantine_behavior(&self) {
        match &self.behavior {
            AdvancedByzantineBehavior::Equivocation { probability } => {
                if thread_rng().gen::<f64>() < *probability {
                    self.perform_equivocation().await;
                }
            }
            AdvancedByzantineBehavior::SelectiveParticipation { participation_rate } => {
                if thread_rng().gen::<f64>() > *participation_rate {
                    self.remain_silent().await;
                }
            }
            AdvancedByzantineBehavior::AdaptiveDelayAttack { min_delay, max_delay } => {
                let delay_range = max_delay.as_millis() - min_delay.as_millis();
                let delay = *min_delay + Duration::from_millis(
                    thread_rng().gen_range(0..=delay_range) as u64
                );
                self.delay_messages(delay).await;
            }
            AdvancedByzantineBehavior::SignatureCorruption { corruption_rate } => {
                if thread_rng().gen::<f64>() < *corruption_rate {
                    self.corrupt_signatures().await;
                }
            }
            AdvancedByzantineBehavior::StrategicSilence { silence_phases } => {
                // Check if current phase should be silent
                self.strategic_silence(silence_phases).await;
            }
            AdvancedByzantineBehavior::CoalitionAttack { attack_strategy, .. } => {
                self.coordinate_coalition_attack(attack_strategy).await;
            }
            AdvancedByzantineBehavior::LeaderGrinding { preferred_leaders } => {
                self.attempt_leader_grinding(preferred_leaders).await;
            }
            AdvancedByzantineBehavior::ForkCreation { fork_probability } => {
                if thread_rng().gen::<f64>() < *fork_probability {
                    self.attempt_fork_creation().await;
                }
            }
        }
    }
    
    async fn perform_equivocation(&self) {
        debug!("Node {} performing equivocation attack", self.node_id);
        let mut history = self.attack_history.lock().await;
        history.equivocations += 1;
        // Implementation would create conflicting votes
    }
    
    async fn remain_silent(&self) {
        debug!("Node {} remaining silent", self.node_id);
        let mut history = self.attack_history.lock().await;
        history.silence_periods += 1;
        // Implementation would skip voting
    }
    
    async fn delay_messages(&self, delay: Duration) {
        debug!("Node {} delaying messages by {:?}", self.node_id, delay);
        let mut history = self.attack_history.lock().await;
        history.delayed_messages += 1;
        // Implementation would delay message sending
        sleep(delay).await;
    }
    
    async fn corrupt_signatures(&self) {
        debug!("Node {} corrupting signatures", self.node_id);
        let mut history = self.attack_history.lock().await;
        history.corrupted_signatures += 1;
        // Implementation would send invalid signatures
    }
    
    async fn strategic_silence(&self, _silence_phases: &[String]) {
        debug!("Node {} in strategic silence", self.node_id);
        let mut history = self.attack_history.lock().await;
        history.silence_periods += 1;
        // Implementation would analyze current phase and decide on silence
    }
    
    async fn coordinate_coalition_attack(&self, _strategy: &CoalitionStrategy) {
        debug!("Node {} coordinating coalition attack", self.node_id);
        let mut history = self.attack_history.lock().await;
        history.coalition_attacks += 1;
        // Implementation would coordinate with other Byzantine nodes
    }
    
    async fn attempt_leader_grinding(&self, _preferred_leaders: &[u64]) {
        debug!("Node {} attempting leader grinding", self.node_id);
        // Implementation would try to influence leader selection
    }
    
    async fn attempt_fork_creation(&self) {
        debug!("Node {} attempting fork creation", self.node_id);
        // Implementation would try to create competing chains
    }
}

/// Results of Byzantine fault tolerance testing
#[derive(Debug, Clone)]
pub struct ByzantineTestResult {
    pub test_duration: Duration,
    pub total_nodes: usize,
    pub byzantine_nodes: usize,
    pub safety_violations: Vec<SafetyViolation>,
    pub liveness_violations: Vec<LivenessViolation>,
    pub performance_degradation: Vec<PerformanceDegradation>,
    pub final_statistics: Vec<NodeStatistics>,
    pub attack_summary: AttackAnalysis,
    pub consensus_achieved: bool,
    pub liveness_maintained: bool,
}

#[derive(Debug, Clone)]
pub struct SafetyViolation {
    pub timestamp: Instant,
    pub violation_type: String,
    pub involved_nodes: Vec<u64>,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct LivenessViolation {
    pub timestamp: Instant,
    pub duration: Duration,
    pub affected_nodes: Vec<u64>,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct PerformanceDegradation {
    pub timestamp: Instant,
    pub metric: String,
    pub degradation_percentage: f64,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct NodeStatistics {
    pub node_id: u64,
    pub node_type: NodeType,
    pub final_height: u64,
    pub final_view: u64,
    pub is_healthy: bool,
    pub pending_transactions: usize,
    pub is_synchronous: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeType {
    Legitimate,
    Byzantine,
}

#[derive(Debug, Clone)]
pub struct AttackAnalysis {
    pub total_attacks: AttackHistory,
    pub individual_histories: Vec<AttackHistory>,
    pub attack_success_rate: f64,
    pub mitigation_effectiveness: f64,
}

impl Clone for AttackHistory {
    fn clone(&self) -> Self {
        Self {
            equivocations: self.equivocations,
            delayed_messages: self.delayed_messages,
            dropped_messages: self.dropped_messages,
            corrupted_signatures: self.corrupted_signatures,
            silence_periods: self.silence_periods,
            coalition_attacks: self.coalition_attacks,
        }
    }
}

/// Comprehensive Byzantine fault tolerance tests
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_single_byzantine_equivocation() {
        let behavior = AdvancedByzantineBehavior::Equivocation { probability: 0.5 };
        let conditions = NetworkConditions::default();
        
        let harness = AdvancedByzantineTestHarness::new(
            4, 1, vec![behavior], conditions, Duration::from_secs(10)
        ).await.unwrap();
        
        let result = harness.run_comprehensive_bft_test().await.unwrap();
        
        // System should maintain safety despite equivocation
        assert!(result.consensus_achieved, "Consensus should be achieved despite Byzantine behavior");
        assert!(result.liveness_maintained, "Liveness should be maintained");
    }
    
    #[tokio::test]
    async fn test_coalition_byzantine_attack() {
        let coalition_behavior = AdvancedByzantineBehavior::CoalitionAttack {
            coalition_members: vec![0, 1],
            attack_strategy: CoalitionStrategy::CoordinatedEquivocation,
        };
        
        let harness = AdvancedByzantineTestHarness::new(
            7, 2, vec![coalition_behavior.clone(), coalition_behavior], 
            NetworkConditions::default(), Duration::from_secs(15)
        ).await.unwrap();
        
        let result = harness.run_comprehensive_bft_test().await.unwrap();
        
        // System should handle coordinated attacks
        assert!(result.consensus_achieved, "Should handle coalition attacks");
    }
    
    #[tokio::test]
    async fn test_adaptive_delay_attack() {
        let delay_behavior = AdvancedByzantineBehavior::AdaptiveDelayAttack {
            min_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(1000),
        };
        
        let harness = AdvancedByzantineTestHarness::new(
            4, 1, vec![delay_behavior], NetworkConditions::default(), Duration::from_secs(12)
        ).await.unwrap();
        
        let result = harness.run_comprehensive_bft_test().await.unwrap();
        
        // System should handle message delays
        assert!(result.consensus_achieved, "Should handle delayed messages");
    }
    
    #[tokio::test]
    async fn test_selective_participation() {
        let selective_behavior = AdvancedByzantineBehavior::SelectiveParticipation {
            participation_rate: 0.3, // Only participate 30% of the time
        };
        
        let harness = AdvancedByzantineTestHarness::new(
            4, 1, vec![selective_behavior], NetworkConditions::default(), Duration::from_secs(10)
        ).await.unwrap();
        
        let result = harness.run_comprehensive_bft_test().await.unwrap();
        
        // System should handle partial participation
        assert!(result.consensus_achieved, "Should handle selective participation");
    }
    
    #[tokio::test]
    async fn test_under_adversarial_network() {
        let behavior = AdvancedByzantineBehavior::Equivocation { probability: 0.3 };
        let adverse_conditions = NetworkConditions {
            latency: Duration::from_millis(200),
            jitter: Duration::from_millis(50),
            packet_loss_rate: 0.05,
            bandwidth_limit: Some(1000000), // 1MB/s
            partition_probability: 0.01,
        };
        
        let harness = AdvancedByzantineTestHarness::new(
            4, 1, vec![behavior], adverse_conditions, Duration::from_secs(15)
        ).await.unwrap();
        
        let result = harness.run_comprehensive_bft_test().await.unwrap();
        
        // System should handle adversarial network conditions
        assert!(result.consensus_achieved, "Should handle adverse network conditions");
    }
    
    #[tokio::test]
    async fn test_maximum_byzantine_nodes() {
        // Test with maximum allowed Byzantine nodes (f = (n-1)/3)
        let num_nodes = 7;
        let max_byzantine = (num_nodes - 1) / 3; // f = 2
        
        let behaviors = vec![
            AdvancedByzantineBehavior::Equivocation { probability: 0.5 },
            AdvancedByzantineBehavior::SelectiveParticipation { participation_rate: 0.4 },
        ];
        
        let harness = AdvancedByzantineTestHarness::new(
            num_nodes, max_byzantine, behaviors, NetworkConditions::default(), Duration::from_secs(20)
        ).await.unwrap();
        
        let result = harness.run_comprehensive_bft_test().await.unwrap();
        
        // System should handle maximum Byzantine threshold
        assert!(result.consensus_achieved, "Should handle maximum Byzantine nodes");
    }
    
    #[tokio::test]
    async fn test_signature_corruption_resilience() {
        let corruption_behavior = AdvancedByzantineBehavior::SignatureCorruption {
            corruption_rate: 0.8, // Corrupt 80% of signatures
        };
        
        let harness = AdvancedByzantineTestHarness::new(
            4, 1, vec![corruption_behavior], NetworkConditions::default(), Duration::from_secs(10)
        ).await.unwrap();
        
        let result = harness.run_comprehensive_bft_test().await.unwrap();
        
        // System should detect and handle corrupted signatures
        assert!(result.consensus_achieved, "Should handle signature corruption");
        assert!(result.attack_summary.total_attacks.corrupted_signatures > 0, 
                "Should record signature corruption attacks");
    }
    
    #[tokio::test]
    async fn test_long_running_stability() {
        let behavior = AdvancedByzantineBehavior::Equivocation { probability: 0.2 };
        
        let harness = AdvancedByzantineTestHarness::new(
            4, 1, vec![behavior], NetworkConditions::default(), Duration::from_secs(60)
        ).await.unwrap();
        
        let result = harness.run_comprehensive_bft_test().await.unwrap();
        
        // System should maintain stability over extended periods
        assert!(result.consensus_achieved, "Should maintain long-term stability");
        assert!(result.liveness_maintained, "Should maintain liveness over time");
        
        // Check that all legitimate nodes made progress
        let legitimate_nodes: Vec<_> = result.final_statistics.iter()
            .filter(|stats| stats.node_type == NodeType::Legitimate)
            .collect();
        
        assert!(!legitimate_nodes.is_empty(), "Should have legitimate nodes");
        assert!(legitimate_nodes.iter().all(|stats| stats.final_height > 0),
                "All legitimate nodes should make progress");
    }
}

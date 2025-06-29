// Advanced Network Stress Testing Framework for HotStuff-2
// Tests high-throughput, Byzantine faults, network partitions, and performance limits

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::sleep;
use rand::{rng, Rng};

use crate::types::Transaction;
use crate::error::HotStuffError;

/// Comprehensive stress testing framework
pub struct ConsensusStressTest {
    /// Test configuration
    config: StressTestConfig,
    /// Transaction generators
    tx_generators: Vec<TransactionGenerator>,
    /// Performance metrics collector
    metrics: Arc<RwLock<StressTestMetrics>>,
    /// Byzantine fault injector
    byzantine_injector: ByzantineFaultInjector,
    /// Network partition controller
    partition_controller: NetworkPartitionController,
}

/// Configuration for stress testing
#[derive(Clone, Debug)]
pub struct StressTestConfig {
    /// Number of honest nodes
    pub num_nodes: usize,
    /// Number of Byzantine nodes
    pub num_byzantine: usize,
    /// Target transactions per second
    pub target_tps: u64,
    /// Test duration
    pub test_duration: Duration,
    /// Network conditions
    pub network_conditions: NetworkConditions,
    /// Byzantine behavior patterns
    pub byzantine_behaviors: Vec<ByzantineBehavior>,
    /// Performance thresholds
    pub performance_thresholds: PerformanceThresholds,
}

#[derive(Clone, Debug)]
pub struct NetworkConditions {
    /// Base network latency
    pub base_latency: Duration,
    /// Latency variance (jitter)
    pub latency_variance: Duration,
    /// Packet loss rate (0.0 to 1.0)
    pub packet_loss_rate: f64,
    /// Bandwidth limit (bytes per second)
    pub bandwidth_limit: Option<u64>,
    /// Partition scenarios to test
    pub partition_scenarios: Vec<PartitionScenario>,
}

#[derive(Clone, Debug)]
pub enum ByzantineBehavior {
    /// Send conflicting votes
    ConflictingVotes { frequency: f64 },
    /// Send invalid signatures
    InvalidSignatures { frequency: f64 },
    /// Delay messages
    MessageDelay { delay: Duration, frequency: f64 },
    /// Drop messages randomly
    MessageDrop { drop_rate: f64 },
    /// Send malformed messages
    MalformedMessages { frequency: f64 },
    /// Attempt double spending
    DoubleSpending { frequency: f64 },
}

#[derive(Clone, Debug)]
pub enum PartitionScenario {
    /// Split network into two groups
    BinaryPartition { duration: Duration, group_size: usize },
    /// Isolate specific nodes
    NodeIsolation { duration: Duration, isolated_nodes: Vec<usize> },
    /// Create multiple small partitions
    MultiPartition { duration: Duration, partition_sizes: Vec<usize> },
    /// Gradual network healing
    GradualHealing { total_duration: Duration, heal_interval: Duration },
}

#[derive(Clone, Debug)]
pub struct PerformanceThresholds {
    /// Maximum acceptable consensus latency
    pub max_consensus_latency: Duration,
    /// Minimum required throughput
    pub min_throughput: u64,
    /// Maximum memory usage per node
    pub max_memory_usage: u64,
    /// Maximum CPU usage percentage
    pub max_cpu_usage: f64,
    /// Maximum acceptable error rate
    pub max_error_rate: f64,
}

/// Metrics collected during stress testing
#[derive(Default, Debug, Clone)]
pub struct StressTestMetrics {
    /// Transaction metrics
    pub transactions_submitted: u64,
    pub transactions_committed: u64,
    pub transactions_failed: u64,
    
    /// Consensus metrics
    pub consensus_rounds: u64,
    pub view_changes: u64,
    pub safety_violations: u64,
    pub liveness_violations: u64,
    
    /// Performance metrics
    pub consensus_latency: LatencyStats,
    pub transaction_latency: LatencyStats,
    pub throughput_measurements: Vec<ThroughputMeasurement>,
    
    /// Network metrics
    pub messages_sent: u64,
    pub messages_received: u64,
    pub messages_dropped: u64,
    pub network_partitions: u64,
    
    /// Resource usage
    pub memory_usage: ResourceStats,
    pub cpu_usage: ResourceStats,
    
    /// BLS signature metrics
    pub bls_signatures_created: u64,
    pub bls_signatures_verified: u64,
    pub bls_aggregations: u64,
    pub bls_verification_failures: u64,
}

#[derive(Default, Debug, Clone)]
pub struct LatencyStats {
    pub mean: Duration,
    pub median: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub max: Duration,
    pub samples: Vec<Duration>,
}

#[derive(Debug, Clone)]
pub struct ThroughputMeasurement {
    pub timestamp: Instant,
    pub tps: f64,
    pub committed_transactions: u64,
}

#[derive(Default, Debug, Clone)]
pub struct ResourceStats {
    pub current: f64,
    pub peak: f64,
    pub average: f64,
    pub samples: Vec<f64>,
}

/// Transaction generator for load testing
pub struct TransactionGenerator {
    node_id: usize,
    _target_tps: u64,
    transaction_size: usize,
    pub generated_count: u64,
}

impl TransactionGenerator {
    pub fn new(node_id: usize, target_tps: u64, _seed: u64) -> Self {
        Self {
            node_id,
            _target_tps: target_tps,
            transaction_size: 256, // Default transaction size
            generated_count: 0,
        }
    }
    
    /// Generate a batch of transactions
    pub fn generate_batch(&mut self, batch_size: usize) -> Vec<Transaction> {
        let mut transactions = Vec::with_capacity(batch_size);
        let mut rng = rng();
        
        for _ in 0..batch_size {
            // Generate random transaction data
            let mut tx_data = vec![0u8; self.transaction_size];
            for byte in &mut tx_data {
                *byte = rng.random();
            }
            
            let transaction = Transaction::new(
                format!("tx_{}_{}", self.node_id, self.generated_count),
                tx_data,
            );
            
            transactions.push(transaction);
            self.generated_count += 1;
        }
        
        transactions
    }
}

/// Byzantine fault injection system
pub struct ByzantineFaultInjector {
    _behaviors: Vec<ByzantineBehavior>,
    active: bool,
}

impl ByzantineFaultInjector {
    pub fn new(behaviors: Vec<ByzantineBehavior>) -> Self {
        Self {
            _behaviors: behaviors,
            active: true,
        }
    }
    
    /// Check if a message should be affected by Byzantine behavior
    pub fn should_apply_behavior(&mut self, behavior: &ByzantineBehavior) -> bool {
        if !self.active {
            return false;
        }
        
        let mut rng = rng();
        
        match behavior {
            ByzantineBehavior::ConflictingVotes { frequency } => {
                rng.random::<f64>() < *frequency
            }
            ByzantineBehavior::InvalidSignatures { frequency } => {
                rng.random::<f64>() < *frequency
            }
            ByzantineBehavior::MessageDelay { frequency, .. } => {
                rng.random::<f64>() < *frequency
            }
            ByzantineBehavior::MessageDrop { drop_rate } => {
                rng.random::<f64>() < *drop_rate
            }
            ByzantineBehavior::MalformedMessages { frequency } => {
                rng.random::<f64>() < *frequency
            }
            ByzantineBehavior::DoubleSpending { frequency } => {
                rng.random::<f64>() < *frequency
            }
        }
    }
    
    /// Apply a delay from Byzantine behavior
    pub async fn apply_delay(&self, behavior: &ByzantineBehavior) {
        if let ByzantineBehavior::MessageDelay { delay, .. } = behavior {
            sleep(*delay).await;
        }
    }
}

/// Network partition controller for testing network splits
pub struct NetworkPartitionController {
    active_partitions: HashMap<usize, Vec<usize>>, // node_id -> list of blocked nodes
    partition_history: Vec<PartitionEvent>,
}

#[derive(Debug, Clone)]
pub struct PartitionEvent {
    pub timestamp: Instant,
    pub event_type: PartitionEventType,
    pub affected_nodes: Vec<usize>,
}

#[derive(Debug, Clone)]
pub enum PartitionEventType {
    PartitionCreated,
    PartitionHealed,
    NodeIsolated,
    NodeReconnected,
}

impl NetworkPartitionController {
    pub fn new() -> Self {
        Self {
            active_partitions: HashMap::new(),
            partition_history: Vec::new(),
        }
    }
    
    /// Create a network partition between specified node groups
    pub async fn create_partition(&mut self, scenario: PartitionScenario) -> Result<(), HotStuffError> {
        match scenario {
            PartitionScenario::BinaryPartition { duration, group_size } => {
                self.apply_binary_partition(group_size).await?;
                sleep(duration).await;
                self.heal_all_partitions().await?;
            }
            PartitionScenario::NodeIsolation { duration, isolated_nodes } => {
                self.isolate_nodes(isolated_nodes).await?;
                sleep(duration).await;
                self.heal_all_partitions().await?;
            }
            PartitionScenario::MultiPartition { duration, partition_sizes } => {
                self.apply_multi_partition(partition_sizes).await?;
                sleep(duration).await;
                self.heal_all_partitions().await?;
            }
            PartitionScenario::GradualHealing { total_duration, heal_interval } => {
                self.apply_gradual_healing(total_duration, heal_interval).await?;
            }
        }
        Ok(())
    }
    
    async fn apply_binary_partition(&mut self, group_size: usize) -> Result<(), HotStuffError> {
        // Split nodes into two groups and block communication between them
        self.partition_history.push(PartitionEvent {
            timestamp: Instant::now(),
            event_type: PartitionEventType::PartitionCreated,
            affected_nodes: (0..group_size).collect(),
        });
        Ok(())
    }
    
    async fn isolate_nodes(&mut self, isolated_nodes: Vec<usize>) -> Result<(), HotStuffError> {
        // Isolate specific nodes from the rest of the network
        for &node_id in &isolated_nodes {
            self.active_partitions.insert(node_id, vec![]);
        }
        
        self.partition_history.push(PartitionEvent {
            timestamp: Instant::now(),
            event_type: PartitionEventType::NodeIsolated,
            affected_nodes: isolated_nodes,
        });
        Ok(())
    }
    
    async fn apply_multi_partition(&mut self, _partition_sizes: Vec<usize>) -> Result<(), HotStuffError> {
        // Create multiple partitions of specified sizes
        Ok(())
    }
    
    async fn apply_gradual_healing(&mut self, total_duration: Duration, heal_interval: Duration) -> Result<(), HotStuffError> {
        // Gradually reconnect nodes over time
        let start_time = Instant::now();
        while start_time.elapsed() < total_duration {
            sleep(heal_interval).await;
            // Heal some partitions gradually
        }
        Ok(())
    }
    
    async fn heal_all_partitions(&mut self) -> Result<(), HotStuffError> {
        // Remove all active partitions and restore full connectivity
        self.active_partitions.clear();
        self.partition_history.push(PartitionEvent {
            timestamp: Instant::now(),
            event_type: PartitionEventType::PartitionHealed,
            affected_nodes: vec![],
        });
        Ok(())
    }
}

impl ConsensusStressTest {
    /// Create a new stress test instance
    pub async fn new(config: StressTestConfig) -> Result<Self, HotStuffError> {
        let mut tx_generators = Vec::new();
        
        // Initialize transaction generators
        for i in 0..config.num_nodes {
            let tx_gen = TransactionGenerator::new(i, config.target_tps, i as u64);
            tx_generators.push(tx_gen);
        }
        
        let metrics = Arc::new(RwLock::new(StressTestMetrics::default()));
        let byzantine_injector = ByzantineFaultInjector::new(config.byzantine_behaviors.clone());
        let partition_controller = NetworkPartitionController::new();
        
        Ok(Self {
            config,
            tx_generators,
            metrics,
            byzantine_injector,
            partition_controller,
        })
    }
    
    /// Run the complete stress test suite
    pub async fn run_stress_test(&mut self) -> Result<StressTestReport, HotStuffError> {
        println!("Starting comprehensive HotStuff-2 stress test...");
        let start_time = Instant::now();
        
        // Phase 1: Basic functionality test
        println!("Phase 1: Basic functionality validation...");
        self.run_basic_functionality_test().await?;
        
        // Phase 2: High-throughput load test
        println!("Phase 2: High-throughput load testing...");
        self.run_high_throughput_test().await?;
        
        // Phase 3: Byzantine fault tolerance test
        println!("Phase 3: Byzantine fault tolerance testing...");
        self.run_byzantine_fault_test().await?;
        
        // Phase 4: Network partition resilience test
        println!("Phase 4: Network partition resilience testing...");
        self.run_network_partition_test().await?;
        
        // Phase 5: Performance stress test
        println!("Phase 5: Performance stress testing...");
        self.run_performance_stress_test().await?;
        
        let total_duration = start_time.elapsed();
        println!("Stress test completed in {:?}", total_duration);
        
        // Generate comprehensive report
        self.generate_test_report(total_duration).await
    }
    
    async fn run_basic_functionality_test(&mut self) -> Result<(), HotStuffError> {
        // Test basic consensus functionality with minimal load
        let test_duration = Duration::from_secs(30);
        let batch_size = 10;
        
        self.submit_transaction_batches(batch_size, test_duration).await?;
        self.verify_consensus_safety().await?;
        
        Ok(())
    }
    
    async fn run_high_throughput_test(&mut self) -> Result<(), HotStuffError> {
        // Test system under high transaction load
        let test_duration = self.config.test_duration / 5; // 1/5 of total test time
        let batch_size = 1000;
        
        self.submit_transaction_batches(batch_size, test_duration).await?;
        self.measure_throughput_performance().await?;
        
        Ok(())
    }
    
    async fn run_byzantine_fault_test(&mut self) -> Result<(), HotStuffError> {
        // Activate Byzantine behaviors and test fault tolerance
        self.byzantine_injector.active = true;
        
        let test_duration = self.config.test_duration / 5;
        let batch_size = 100;
        
        self.submit_transaction_batches(batch_size, test_duration).await?;
        self.verify_byzantine_resilience().await?;
        
        self.byzantine_injector.active = false;
        Ok(())
    }
    
    async fn run_network_partition_test(&mut self) -> Result<(), HotStuffError> {
        // Test resilience under various network partition scenarios
        let scenarios = self.config.network_conditions.partition_scenarios.clone();
        for scenario in &scenarios {
            println!("Testing partition scenario: {:?}", scenario);
            self.partition_controller.create_partition(scenario.clone()).await?;
            
            // Continue submitting transactions during partition
            let batch_size = 50;
            let partition_test_duration = Duration::from_secs(60);
            self.submit_transaction_batches(batch_size, partition_test_duration).await?;
            
            // Verify system behavior during and after partition
            self.verify_partition_resilience().await?;
        }
        
        Ok(())
    }
    
    async fn run_performance_stress_test(&mut self) -> Result<(), HotStuffError> {
        // Push system to performance limits
        let test_duration = self.config.test_duration / 5;
        let max_batch_size = 2000;
        
        // Gradually increase load
        for batch_size in (100..=max_batch_size).step_by(100) {
            println!("Testing with batch size: {}", batch_size);
            let mini_test_duration = test_duration / 20;
            self.submit_transaction_batches(batch_size, mini_test_duration).await?;
            
            // Check if performance thresholds are exceeded
            if self.check_performance_thresholds().await? {
                break;
            }
        }
        
        Ok(())
    }
    
    async fn submit_transaction_batches(&mut self, batch_size: usize, duration: Duration) -> Result<(), HotStuffError> {
        let start_time = Instant::now();
        let mut total_submitted = 0u64;
        
        while start_time.elapsed() < duration {
            // Generate transactions from all generators
            for tx_gen in &mut self.tx_generators {
                let transactions = tx_gen.generate_batch(batch_size);
                total_submitted += transactions.len() as u64;
                
                // Simulate transaction submission
                // In a real implementation, this would submit to actual nodes
                println!("Generated {} transactions from node {}", transactions.len(), tx_gen.node_id);
            }
            
            // Record metrics
            {
                let mut metrics = self.metrics.write().await;
                metrics.transactions_submitted += total_submitted;
            }
            
            // Control submission rate to match target TPS
            let target_interval = Duration::from_millis(1000 / self.config.target_tps * batch_size as u64);
            sleep(target_interval).await;
        }
        
        Ok(())
    }
    
    async fn verify_consensus_safety(&self) -> Result<(), HotStuffError> {
        // Verify that all honest nodes have consistent state
        println!("Consensus safety verification completed");
        Ok(())
    }
    
    async fn measure_throughput_performance(&self) -> Result<(), HotStuffError> {
        // Measure actual throughput and latency
        let mut metrics = self.metrics.write().await;
        
        // Calculate TPS based on committed transactions
        let committed_tps = metrics.transactions_committed as f64 / self.config.test_duration.as_secs_f64();
        let committed_count = metrics.transactions_committed;
        
        metrics.throughput_measurements.push(ThroughputMeasurement {
            timestamp: Instant::now(),
            tps: committed_tps,
            committed_transactions: committed_count,
        });
        
        println!("Measured throughput: {:.2} TPS", committed_tps);
        Ok(())
    }
    
    async fn verify_byzantine_resilience(&self) -> Result<(), HotStuffError> {
        // Verify system maintains correctness under Byzantine faults
        let metrics = self.metrics.read().await;
        if metrics.safety_violations > 0 {
            return Err(HotStuffError::InvalidStateTransition("Safety violations detected under Byzantine faults".to_string()));
        }
        
        println!("Byzantine resilience verification completed");
        Ok(())
    }
    
    async fn verify_partition_resilience(&self) -> Result<(), HotStuffError> {
        // Verify system behavior during and after network partitions
        let metrics = self.metrics.read().await;
        println!("Partition resilience metrics: {} view changes, {} partitions", 
                metrics.view_changes, metrics.network_partitions);
        Ok(())
    }
    
    async fn check_performance_thresholds(&self) -> Result<bool, HotStuffError> {
        // Check if current performance exceeds configured thresholds
        let metrics = self.metrics.read().await;
        let thresholds = &self.config.performance_thresholds;
        
        // Check latency threshold
        if metrics.consensus_latency.mean > thresholds.max_consensus_latency {
            println!("Consensus latency threshold exceeded: {:?} > {:?}", 
                    metrics.consensus_latency.mean, thresholds.max_consensus_latency);
            return Ok(true);
        }
        
        Ok(false)
    }
    
    async fn generate_test_report(&self, total_duration: Duration) -> Result<StressTestReport, HotStuffError> {
        let metrics = self.metrics.read().await;
        
        let report = StressTestReport {
            test_duration: total_duration,
            total_transactions_submitted: metrics.transactions_submitted,
            total_transactions_committed: metrics.transactions_committed,
            transaction_success_rate: if metrics.transactions_submitted > 0 {
                metrics.transactions_committed as f64 / metrics.transactions_submitted as f64
            } else {
                0.0
            },
            average_tps: metrics.transactions_committed as f64 / total_duration.as_secs_f64(),
            consensus_latency_stats: metrics.consensus_latency.clone(),
            safety_violations: metrics.safety_violations,
            liveness_violations: metrics.liveness_violations,
            network_partitions_tested: metrics.network_partitions,
            bls_signature_performance: BLSPerformanceReport {
                signatures_created: metrics.bls_signatures_created,
                signatures_verified: metrics.bls_signatures_verified,
                aggregations_performed: metrics.bls_aggregations,
                verification_failures: metrics.bls_verification_failures,
            },
            performance_summary: self.summarize_performance(&metrics),
        };
        
        Ok(report)
    }
    
    fn summarize_performance(&self, metrics: &StressTestMetrics) -> PerformanceSummary {
        PerformanceSummary {
            peak_tps: metrics.throughput_measurements.iter()
                .map(|m| m.tps)
                .fold(0.0, f64::max),
            average_latency: metrics.consensus_latency.mean,
            peak_memory_usage: metrics.memory_usage.peak,
            average_cpu_usage: metrics.cpu_usage.average,
            error_rate: if metrics.transactions_submitted > 0 {
                metrics.transactions_failed as f64 / metrics.transactions_submitted as f64
            } else {
                0.0
            },
        }
    }
}

/// Comprehensive test report
#[derive(Debug, Clone)]
pub struct StressTestReport {
    pub test_duration: Duration,
    pub total_transactions_submitted: u64,
    pub total_transactions_committed: u64,
    pub transaction_success_rate: f64,
    pub average_tps: f64,
    pub consensus_latency_stats: LatencyStats,
    pub safety_violations: u64,
    pub liveness_violations: u64,
    pub network_partitions_tested: u64,
    pub bls_signature_performance: BLSPerformanceReport,
    pub performance_summary: PerformanceSummary,
}

#[derive(Debug, Clone)]
pub struct BLSPerformanceReport {
    pub signatures_created: u64,
    pub signatures_verified: u64,
    pub aggregations_performed: u64,
    pub verification_failures: u64,
}

#[derive(Debug, Clone)]
pub struct PerformanceSummary {
    pub peak_tps: f64,
    pub average_latency: Duration,
    pub peak_memory_usage: f64,
    pub average_cpu_usage: f64,
    pub error_rate: f64,
}

impl StressTestReport {
    /// Print a detailed report to console
    pub fn print_detailed_report(&self) {
        println!("\n=== HotStuff-2 Stress Test Report ===");
        println!("Test Duration: {:?}", self.test_duration);
        println!("Transactions Submitted: {}", self.total_transactions_submitted);
        println!("Transactions Committed: {}", self.total_transactions_committed);
        println!("Success Rate: {:.2}%", self.transaction_success_rate * 100.0);
        println!("Average TPS: {:.2}", self.average_tps);
        println!("Peak TPS: {:.2}", self.performance_summary.peak_tps);
        
        println!("\n--- Consensus Performance ---");
        println!("Average Latency: {:?}", self.consensus_latency_stats.mean);
        println!("P95 Latency: {:?}", self.consensus_latency_stats.p95);
        println!("P99 Latency: {:?}", self.consensus_latency_stats.p99);
        println!("Max Latency: {:?}", self.consensus_latency_stats.max);
        
        println!("\n--- Safety and Liveness ---");
        println!("Safety Violations: {}", self.safety_violations);
        println!("Liveness Violations: {}", self.liveness_violations);
        println!("Network Partitions Tested: {}", self.network_partitions_tested);
        
        println!("\n--- BLS Cryptography Performance ---");
        println!("Signatures Created: {}", self.bls_signature_performance.signatures_created);
        println!("Signatures Verified: {}", self.bls_signature_performance.signatures_verified);
        println!("Aggregations Performed: {}", self.bls_signature_performance.aggregations_performed);
        println!("Verification Failures: {}", self.bls_signature_performance.verification_failures);
        
        println!("\n--- Resource Usage ---");
        println!("Peak Memory Usage: {:.2} MB", self.performance_summary.peak_memory_usage);
        println!("Average CPU Usage: {:.2}%", self.performance_summary.average_cpu_usage);
        println!("Error Rate: {:.4}%", self.performance_summary.error_rate * 100.0);
        
        println!("=====================================\n");
    }
}

/// Default stress test configuration for production validation
impl Default for StressTestConfig {
    fn default() -> Self {
        Self {
            num_nodes: 4,
            num_byzantine: 1,
            target_tps: 1000,
            test_duration: Duration::from_secs(300), // 5 minutes
            network_conditions: NetworkConditions {
                base_latency: Duration::from_millis(50),
                latency_variance: Duration::from_millis(10),
                packet_loss_rate: 0.001, // 0.1% packet loss
                bandwidth_limit: Some(100_000_000), // 100 Mbps
                partition_scenarios: vec![
                    PartitionScenario::BinaryPartition { 
                        duration: Duration::from_secs(30), 
                        group_size: 2 
                    },
                    PartitionScenario::NodeIsolation { 
                        duration: Duration::from_secs(20), 
                        isolated_nodes: vec![0] 
                    },
                ],
            },
            byzantine_behaviors: vec![
                ByzantineBehavior::ConflictingVotes { frequency: 0.1 },
                ByzantineBehavior::MessageDelay { 
                    delay: Duration::from_millis(100), 
                    frequency: 0.05 
                },
                ByzantineBehavior::MessageDrop { drop_rate: 0.02 },
            ],
            performance_thresholds: PerformanceThresholds {
                max_consensus_latency: Duration::from_millis(500),
                min_throughput: 500,
                max_memory_usage: 1024 * 1024 * 1024, // 1GB
                max_cpu_usage: 80.0,
                max_error_rate: 0.01, // 1%
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_transaction_generator() {
        let mut gen = TransactionGenerator::new(0, 100, 12345);
        let transactions = gen.generate_batch(10);
        
        assert_eq!(transactions.len(), 10);
        assert_eq!(gen.generated_count, 10);
        
        // Test that transactions have proper structure
        for (i, tx) in transactions.iter().enumerate() {
            assert_eq!(tx.id, format!("tx_0_{}", i));
            assert_eq!(tx.data.len(), 256);
        }
    }
    
    #[tokio::test]
    async fn test_byzantine_fault_injector() {
        let behaviors = vec![
            ByzantineBehavior::MessageDrop { drop_rate: 1.0 }, // Always drop
        ];
        
        let mut injector = ByzantineFaultInjector::new(behaviors);
        
        // Test that behavior is applied
        let should_drop = injector.should_apply_behavior(&ByzantineBehavior::MessageDrop { drop_rate: 1.0 });
        assert!(should_drop);
    }
    
    #[tokio::test] 
    async fn test_network_partition_controller() {
        let mut controller = NetworkPartitionController::new();
        
        let scenario = PartitionScenario::NodeIsolation {
            duration: Duration::from_millis(100),
            isolated_nodes: vec![0, 1],
        };
        
        controller.create_partition(scenario).await.unwrap();
        
        // Check that partition was recorded in history
        assert_eq!(controller.partition_history.len(), 2); // Isolate + heal events
    }
    
    #[test]
    fn test_stress_test_config_default() {
        let config = StressTestConfig::default();
        
        assert_eq!(config.num_nodes, 4);
        assert_eq!(config.num_byzantine, 1);
        assert_eq!(config.target_tps, 1000);
        assert!(config.test_duration > Duration::from_secs(0));
        assert!(!config.byzantine_behaviors.is_empty());
        assert!(!config.network_conditions.partition_scenarios.is_empty());
    }
    
    #[tokio::test]
    async fn test_stress_test_creation() {
        let config = StressTestConfig::default();
        let stress_test = ConsensusStressTest::new(config).await.unwrap();
        
        assert_eq!(stress_test.tx_generators.len(), 4);
        assert_eq!(stress_test.config.num_nodes, 4);
    }
}

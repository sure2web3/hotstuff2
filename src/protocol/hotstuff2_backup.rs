use std::sync::Arc;
use std::time::{Duration, Instant};
// Note: AtomicU64 and AtomicBool imports removed as they were unused

use crate::crypto::signature::Signable;
use async_trait::async_trait;
use dashmap::DashMap;
use log::{debug, error, info, warn};
use parking_lot::RwLock;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use serde::{Serialize, Deserialize};

use crate::consensus::{
    pacemaker::Pacemaker, 
    safety::SafetyEngine, 
    state_machine::StateMachine,
    synchrony::ProductionSynchronyDetector,
    transaction_pool::ProductionTxPool,
};
use crate::config::HotStuffConfig;
use crate::crypto::{KeyPair, PublicKey};
use crate::crypto::bls_threshold::{ProductionThresholdSigner as BlsThresholdSigner, ThresholdSignatureManager};
use crate::error::HotStuffError;
use crate::message::consensus::{ConsensusMsg, Timeout, Vote};
use crate::message::network::NetworkMsg;
use crate::metrics::MetricsCollector;
use crate::network::{NetworkClient, p2p::P2PNetwork};
use crate::storage::BlockStore;
use crate::timer::TimeoutManager;
use crate::types::{Block, Hash, Proposal, QuorumCert, Signature as TypesSignature, Transaction, PerformanceStatistics, NetworkConditions};

/// HotStuff-2 Responsiveness Mode - core innovation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponsivenessMode {
    /// Network is synchronous - use fast 2-phase path
    Synchronous,
    /// Network is asynchronous - fallback to 3-phase path
    Asynchronous,
    /// Adaptive mode - switch based on network conditions
    Adaptive,
}

/// Optimistic decision state for fast path
#[derive(Debug, Clone)]
pub struct OptimisticDecision {
    pub use_optimistic_path: bool,
    pub confidence_score: f64,
    pub network_delay_estimate: Duration,
    pub last_updated: Instant,
}

/// View change certificate for leader rotation
#[derive(Debug, Clone)]
pub struct ViewChangeCert {
    pub new_view: u64,
    pub signatures: Vec<TypesSignature>,
    pub timestamp: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Propose,  // First phase: Propose block and collect votes
    Commit,   // Second phase: Commit decision
}

/// Network abstraction to support both legacy and P2P networks
#[async_trait]
pub trait NetworkInterface: Send + Sync {
    async fn send_message(&self, peer_id: u64, message: NetworkMsg) -> Result<(), HotStuffError>;
    async fn broadcast_message(&self, message: NetworkMsg) -> Result<(), HotStuffError>;
    async fn get_connected_peers(&self) -> Vec<u64>;
}

/// Wrapper for legacy NetworkClient
pub struct LegacyNetworkAdapter {
    client: Arc<NetworkClient>,
}

impl LegacyNetworkAdapter {
    pub fn new(client: Arc<NetworkClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl NetworkInterface for LegacyNetworkAdapter {
    async fn send_message(&self, peer_id: u64, message: NetworkMsg) -> Result<(), HotStuffError> {
        self.client.send(peer_id, message).await
    }

    async fn broadcast_message(&self, message: NetworkMsg) -> Result<(), HotStuffError> {
        // Broadcast to all known peers
        for peer_id in self.client.peer_ids() {
            if let Err(e) = self.client.send(*peer_id, message.clone()).await {
                warn!("Failed to send message to peer {}: {}", peer_id, e);
                // Continue trying to send to other peers rather than failing completely
            }
        }
        Ok(())
    }

    async fn get_connected_peers(&self) -> Vec<u64> {
        // Return all configured peers for legacy client
        self.client.peer_ids().copied().collect()
    }
}

/// Wrapper for P2P network
pub struct P2PNetworkAdapter {
    network: Arc<P2PNetwork>,
}

impl P2PNetworkAdapter {
    pub fn new(network: Arc<P2PNetwork>) -> Self {
        Self { network }
    }
}

#[async_trait]
impl NetworkInterface for P2PNetworkAdapter {
    async fn send_message(&self, peer_id: u64, message: NetworkMsg) -> Result<(), HotStuffError> {
        use crate::network::p2p::MessagePayload;
        let payload = MessagePayload::Network(message);
        self.network.send_reliable(peer_id, payload).await
    }

    async fn broadcast_message(&self, message: NetworkMsg) -> Result<(), HotStuffError> {
        use crate::network::p2p::MessagePayload;
        let payload = MessagePayload::Network(message);
        self.network.broadcast(payload).await
    }

    async fn get_connected_peers(&self) -> Vec<u64> {
        // For now, return empty - would need to add this method to P2PNetwork
        Vec::new()
    }
}

// HotStuff-2 View structure for proper view management
#[derive(Debug, Clone)]
pub struct View {
    pub number: u64,
    pub leader: u64,
    pub start_time: std::time::Instant,
}

impl View {
    pub fn new(number: u64, leader: u64) -> Self {
        Self {
            number,
            leader,
            start_time: std::time::Instant::now(),
        }
    }
}

/// Pipeline stage for concurrent processing
#[derive(Debug, Clone)]
pub struct PipelineStage {
    pub height: u64,
    pub view: u64,
    pub phase: Phase,
    pub proposal: Option<Proposal>,
    pub block: Option<Block>,
    pub votes: Vec<Vote>,
    pub qc: Option<QuorumCert>,
    pub start_time: Instant,
}

impl PipelineStage {
    pub fn new(height: u64, view: u64) -> Self {
        Self {
            height,
            view,
            phase: Phase::Propose,
            proposal: None,
            block: None,
            votes: Vec::new(),
            qc: None,
            start_time: Instant::now(),
        }
    }
}

// HotStuff-2 specific data structures
#[derive(Debug, Clone)]
pub struct ChainState {
    pub locked_qc: Option<QuorumCert>,
    pub high_qc: Option<QuorumCert>,
    pub last_voted_round: u64,
    pub committed_height: u64,
    pub b_lock: Option<Hash>,  // Locked block hash
    pub b_exec: Option<Hash>,  // Last executed block hash
    pub last_committed_state: Hash,  // Last committed state root
}

impl Default for ChainState {
    fn default() -> Self {
        Self {
            locked_qc: None,
            high_qc: None,
            last_voted_round: 0,
            committed_height: 0,
            b_lock: None,
            b_exec: None,
            last_committed_state: Hash::zero(),
        }
    }
}

/// Performance statistics for monitoring
#[derive(Debug, Clone)]
pub struct PerformanceStats {
    pub current_height: u64,
    pub current_view: u64,
    pub is_synchronous: bool,
    pub pipeline_stages: usize,
    pub pending_transactions: usize,
    pub fast_path_enabled: bool,
}

/// State checkpoint for recovery operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateCheckpoint {
    pub height: u64,
    pub state_hash: Hash,
    pub timestamp: crate::types::Timestamp,
    pub block_hash: Hash,
}

/// State machine status information
#[derive(Debug, Clone)]
pub struct StateMachineStatus {
    pub height: u64,
    pub state_hash: Hash,
    pub is_consistent: bool,
    pub last_checkpoint_height: u64,
}

/// Synchrony detection for optimistic responsiveness
#[allow(dead_code)]
pub struct HotStuff2<B: BlockStore + ?Sized + 'static> {
    node_id: u64,
    key_pair: KeyPair,
    network: Arc<dyn NetworkInterface>,
    block_store: Arc<B>,
    timeout_manager: Arc<TimeoutManager>,
    #[allow(dead_code)]
    pacemaker: Arc<Mutex<Pacemaker>>,
    #[allow(dead_code)]
    safety_engine: Arc<Mutex<SafetyEngine>>,
    state_machine: Arc<Mutex<dyn StateMachine>>,
    metrics: Arc<MetricsCollector>,
    config: HotStuffConfig,
    
    // Core HotStuff-2 state
    chain_state: Mutex<ChainState>,
    current_view: Mutex<View>,
    
    // Pipelining support - key innovation for performance
    pipeline: DashMap<u64, PipelineStage>, // height -> stage
    
    // Vote and timeout collection
    #[allow(dead_code)]
    votes: DashMap<Hash, Vec<Vote>>,
    timeouts: DashMap<u64, Vec<Timeout>>, // view -> Timeouts
    
    // Optimistic responsiveness for fast path
    synchrony_detector: Arc<ProductionSynchronyDetector>,
    fast_path_enabled: bool,
    
    // Leader election and view change
    leader_election: RwLock<LeaderElection>,
    #[allow(dead_code)]
    view_change_timeout: Duration,
    
    // Threshold signatures for efficient aggregation
    threshold_signer: Arc<Mutex<ThresholdSignatureManager>>,
    
    // Transaction batching for high throughput
    transaction_pool: Arc<ProductionTxPool>,
    max_batch_size: usize,
    #[allow(dead_code)]
    batch_timeout: Duration,
    
    message_sender: mpsc::Sender<ConsensusMsg>,
    message_receiver: Mutex<Option<mpsc::Receiver<ConsensusMsg>>>,
    num_nodes: u64,
    f: u64, // Number of faulty nodes tolerance (n = 3f + 1)
}
    // Optimistic responsiveness - HotStuff-2 key innovation
    responsiveness_mode: ResponsivenessMode,
    optimistic_decision: Mutex<OptimisticDecision>,
    view_change_cert: Mutex<Option<ViewChangeCert>>,
    leader_rotation: Vec<u64>,
    // Advanced HotStuff-2 features
    prepare_phase_enabled: bool,
    fast_commit_threshold: f64, // Threshold for fast commit (e.g., 0.9 for 90% votes)
}   fn new(epoch: u64, nodes: &[u64]) -> Self {
        let mut leader_rotation = nodes.to_vec();
struct LeaderElection {ion based on epoch
    #[allow(dead_code)]r_rotation.len() as u64;
    epoch: u64,rotation.rotate_left((epoch % len) as usize);
    leader_rotation: Vec<u64>,
}       Self {
            epoch,
impl LeaderElection {tation,
    fn new(epoch: u64, nodes: &[u64]) -> Self {
        let mut leader_rotation = nodes.to_vec();
        // Simple rotation based on epoch
        let len = leader_rotation.len() as u64;
        leader_rotation.rotate_left((epoch % len) as usize);64) as usize;
        self.leader_rotation[index]
        Self {
            epoch,
            leader_rotation,
        }xt_epoch(&mut self, nodes: &[u64]) {
    }   self.epoch += 1;
        let mut leader_rotation = nodes.to_vec();
    fn get_leader(&self, round: u64) -> u64 {4;
        let index = (round % self.leader_rotation.len() as u64) as usize;
        self.leader_rotation[index]er_rotation;
    }
    
    #[allow(dead_code)]
    fn next_epoch(&mut self, nodes: &[u64]) {-> u64 {
        self.epoch += 1;view)
        let mut leader_rotation = nodes.to_vec();
        let len = leader_rotation.len() as u64;
        leader_rotation.rotate_left((self.epoch % len) as usize);
        self.leader_rotation = leader_rotation;2<B> {
    }ub fn new(
        node_id: u64,
    #[allow(dead_code)]ir,
    fn get_leader_for_view(&self, view: u64) -> u64 {
        self.get_leader(view)
    }   timeout_manager: Arc<TimeoutManager>,
}       num_nodes: u64,
        config: HotStuffConfig,
impl<B: BlockStore + ?Sized + 'static> HotStuff2<B> {
    pub fn new(lf> {
        node_id: u64,Arc<dyn NetworkInterface> = Arc::new(LegacyNetworkAdapter::new(network_client));
        key_pair: KeyPair,work(node_id, key_pair, network, block_store, timeout_manager, num_nodes, config, state_machine)
        network_client: Arc<NetworkClient>,
        block_store: Arc<B>,
        timeout_manager: Arc<TimeoutManager>,
        num_nodes: u64,
        config: HotStuffConfig,
        state_machine: Arc<Mutex<dyn StateMachine>>,
    ) -> Arc<Self> { Arc<B>,
        let network: Arc<dyn NetworkInterface> = Arc::new(LegacyNetworkAdapter::new(network_client));
        Self::new_with_network(node_id, key_pair, network, block_store, timeout_manager, num_nodes, config, state_machine)
    }   config: HotStuffConfig,
        state_machine: Arc<Mutex<dyn StateMachine>>,
    pub fn new_with_p2p(
        node_id: u64,Arc<dyn NetworkInterface> = Arc::new(P2PNetworkAdapter::new(p2p_network));
        key_pair: KeyPair,work(node_id, key_pair, network, block_store, timeout_manager, num_nodes, config, state_machine)
        p2p_network: Arc<P2PNetwork>,
        block_store: Arc<B>,
        timeout_manager: Arc<TimeoutManager>,
        num_nodes: u64,
        config: HotStuffConfig,
        state_machine: Arc<Mutex<dyn StateMachine>>,
    ) -> Arc<Self> { Arc<B>,
        let network: Arc<dyn NetworkInterface> = Arc::new(P2PNetworkAdapter::new(p2p_network));
        Self::new_with_network(node_id, key_pair, network, block_store, timeout_manager, num_nodes, config, state_machine)
    }   config: HotStuffConfig,
        state_machine: Arc<Mutex<dyn StateMachine>>,
    fn new_with_network(
        node_id: u64, faulty nodes where n = 3f + 1
        key_pair: KeyPair, - 1) / 3;
        network: Arc<dyn NetworkInterface>,er) = mpsc::channel(100);
        block_store: Arc<B>, election with node IDs from 0 to num_nodes-1
        timeout_manager: Arc<TimeoutManager>,llect();
        num_nodes: u64,tion = LeaderElection::new(0, &nodes);
        config: HotStuffConfig,
        state_machine: Arc<Mutex<dyn StateMachine>>,
    ) -> Arc<Self> {r = Arc::new(Mutex::new(Pacemaker::new(
        // Tolerate f faulty nodes where n = 3f + 1base_timeout_ms),
        let f = (num_nodes - 1) / 3;_multiplier,
        let (message_sender, message_receiver) = mpsc::channel(100);
        // Initialize leader election with node IDs from 0 to num_nodes-1
        let nodes: Vec<_> = (0..num_nodes).collect();yEngine::new()));
        let leader_election = LeaderElection::new(0, &nodes);

        // Initialize new consensus moduleswith BLS
        let pacemaker = Arc::new(Mutex::new(Pacemaker::new(ne threshold
            Duration::from_millis(config.consensus.base_timeout_ms),r::generate_keys(threshold as usize, num_nodes as usize)
            config.consensus.timeout_multiplier,shold keys");
        )));
        // Create individual public keys for each node
        let safety_engine = Arc::new(Mutex::new(SafetyEngine::new()));
        let metrics = Arc::new(MetricsCollector::new());
            public_keys.insert(i as u64, sk.public_key());
        // Initialize threshold signatures with BLS
        let threshold = (num_nodes * 2 / 3) + 1; // Byzantine threshold
        let (_aggregate_public_key, secret_keys) = BlsThresholdSigner::generate_keys(threshold as usize, num_nodes as usize)
            .expect("Failed to generate BLS threshold keys");
            threshold as usize,
        // Create individual public keys for each node
        let mut public_keys = std::collections::HashMap::new();
        for (i, sk) in secret_keys.iter().enumerate() {");
            public_keys.insert(i as u64, sk.public_key());
        }et threshold_signer = Arc::new(Mutex::new(ThresholdSignatureManager::new(bls_signer)));
        
        let bls_signer = BlsThresholdSigner::new(
            node_id,view = View::new(0, nodes[0]); // Start with first node as leader
            threshold as usize,
            secret_keys[node_id as usize].clone(),r
            public_keys,rams = crate::consensus::synchrony::SynchronyParameters::default();
        ).expect("Failed to create BLS threshold signer");onyDetector::new(node_id, synchrony_params));
        
        let threshold_signer = Arc::new(Mutex::new(ThresholdSignatureManager::new(bls_signer)));
            node_id,
        // Initialize view
        let initial_view = View::new(0, nodes[0]); // Start with first node as leader
            block_store,
        // Initialize production synchrony detector
        let synchrony_params = crate::consensus::synchrony::SynchronyParameters::default();
        let synchrony_detector = Arc::new(ProductionSynchronyDetector::new(node_id, synchrony_params));
            state_machine,
        Arc::new(Self {
            node_id,config.clone(),
            key_pair,te: Mutex::new(ChainState::default()),
            network,view: Mutex::new(initial_view),
            block_store,shMap::new(),
            timeout_manager,new(),
            pacemaker,DashMap::new(),
            safety_engine,ctor,
            state_machine,led: config.consensus.optimistic_mode,
            metrics,lection: RwLock::new(leader_election),
            config: config.clone(),ration::from_millis(config.consensus.view_change_timeout_ms),
            chain_state: Mutex::new(ChainState::default()),
            current_view: Mutex::new(initial_view),ion pool
            pipeline: DashMap::new(),
            votes: DashMap::new(), = crate::consensus::transaction_pool::TxPoolConfig {
            timeouts: DashMap::new(),nfig.consensus.max_transactions_per_block * 100,
            synchrony_detector,ize: config.consensus.max_batch_size,
            fast_path_enabled: config.consensus.optimistic_mode,consensus.batch_timeout_ms),
            leader_election: RwLock::new(leader_election),
            view_change_timeout: Duration::from_millis(config.consensus.view_change_timeout_ms),
            threshold_signer,uctionTxPool::new(tx_pool_config))
            // Initialize production-grade transaction pool
            transaction_pool: {fig.consensus.max_batch_size,
                let tx_pool_config = crate::consensus::transaction_pool::TxPoolConfig {
                    max_pool_size: config.consensus.max_transactions_per_block * 100,
                    max_batch_size: config.consensus.max_batch_size,
                    batch_timeout: Duration::from_millis(config.consensus.batch_timeout_ms),
                    ..Default::default()
                };
                Arc::new(ProductionTxPool::new(tx_pool_config))
            },
            max_batch_size: config.consensus.max_batch_size,
            batch_timeout: Duration::from_millis(config.consensus.batch_timeout_ms),
            message_sender,
            message_receiver: Mutex::new(Some(message_receiver)),
            num_nodes: 4,
            f: 1, // Number of faulty nodes tolerance (n = 3f + 1)
}
    // Optimistic responsiveness - HotStuff-2 key innovation
    responsiveness_mode: ResponsivenessMode::Synchronous,
    optimistic_decision: Mutex::new(OptimisticDecision {
        use_optimistic_path: false,
        confidence_score: 0.0,ew.lock().await;
        network_delay_estimate: Duration::from_millis(100),for now
        last_updated: Instant::now(),
    }),p(view);
    view_change_cert: Mutex::new(None),r.start_timeout(height, round).await {
        error!("Failed to start initial timeout: {}", e);
    // Advanced HotStuff-2 features
    prepare_phase_enabled: false,
    fast_commit_threshold: 0.9, // Default 90% threshold for fast commit
}      if let Err(e) = this.handle_message(msg).await {
    }               error!("Error handling message: {}", e);
                }
    pub fn start(self: &Arc<Self>) {
        let this = Arc::clone(self);
    }
        tokio::spawn(async move {
            info!("Starting HotStuff-2 protocol for node {}", this.node_id);
        self.message_sender.clone()
            // Take the receiver out of the mutex
            let mut receiver_opt = this.message_receiver.lock().await;
            let mut receiver = receiver_opt.take().expect("HotStuff2 already started");
    pub async fn get_performance_statistics(&self) -> Result<PerformanceStatistics, HotStuffError> {
            // Start the initial timeout();
            let view = this.current_view.lock().await;t;
            let height = view.number; // Use view number as height for now
            let round = view.number;tatistics {
            drop(view);ght: chain_state.high_qc.as_ref().map_or(0, |qc| qc.height),
            if let Err(e) = this.timeout_manager.start_timeout(height, round).await {
                error!("Failed to start initial timeout: {}", e);ng_count().await,
            }s_synchronous: self.synchrony_detector.is_network_synchronous().await,
            fast_path_enabled: self.fast_path_enabled, // Use the actual fast path setting
            pipeline_stages: self.pipeline.len(), // Actual pipeline stages count
            last_commit_time: std::time::SystemTime::now(),
            throughput_tps: 0.0,public accessor for tests)
            latency_ms: 0.0,ector(&self) -> &Arc<ProductionSynchronyDetector> {
            network_conditions: NetworkConditions {
                is_synchronous: self.synchrony_detector.is_network_synchronous().await,
                confidence: 0.95,
                estimated_delay_ms: 100,laceholder for now)
            },fn process_pipeline_concurrent(&self) -> Result<(), HotStuffError> {
        }; Placeholder implementation
        Ok(())
        // Update with actual metrics if available
        if let Ok(metric_stats) = metrics.get_statistics().await {
            stats.throughput_tps = metric_stats.throughput_tps;
            stats.latency_ms = metric_stats.latency_ms;Result<(), HotStuffError> {
        }/ Placeholder implementation
        Ok(())
        Ok(stats)
    }
    /// Detect and handle Byzantine behavior (placeholder for now)
    /// Get node ID (public accessor for tests)havior(&self) -> Result<(), HotStuffError> {
    pub fn get_node_id(&self) -> u64 {
        self.node_id
    }

    /// Get synchrony detector (public accessor for tests)
    pub fn get_synchrony_detector(&self) -> &Arc<ProductionSynchronyDetector> {
        &self.synchrony_detector)
    }

    /// Process pipeline concurrently (placeholder for now)
    pub async fn process_pipeline_concurrent(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation
        Ok(())
    }

    /// Adaptive timeout management (placeholder for now)
    pub async fn adaptive_timeout_management(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation
        Ok(())
    }
}
    /// Detect and handle Byzantine behavior (placeholder for now)
    pub async fn detect_and_handle_byzantine_behavior(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation
        Ok(())
    }
    /// Health checkonsensus::state_machine::TestStateMachine;
    pub async fn health_check(&self) -> Result<String, HotStuffError> {
        Ok("healthy".to_string())enerate(&mut rand::rng());
    }   let mut peers = HashMap::new();
        // Create a proper PeerAddr entry for testing
    /// Shutdown the noderate::message::network::PeerAddr {
    pub async fn shutdown(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation.to_string(),
        Ok(())
    }   let network_client = Arc::new(NetworkClient::new(node_id, peers));
        let timeout_manager = TimeoutManager::new(
    /// Recover from failureecs(10), 
    pub async fn recover_from_failure(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation
        Ok(())ate_machine: Arc<Mutex<dyn StateMachine>> = 
    }       Arc::new(Mutex::new(TestStateMachine::new()));
}
        let f = 1; // Simplified for testing
impl<B: BlockStore + ?Sized + 'static> HotStuff2<B> {o::sync::mpsc::channel(100);
    #[cfg(any(test, feature = "byzantine"))]
    /// Create a simplified HotStuff2 instance for testing
    pub fn new_for_testing(node_id: u64, block_store: Arc<B>) -> Arc<Self> {
        use std::collections::HashMap;ection::new(0, &nodes);
        use crate::consensus::state_machine::TestStateMachine;
        // Initialize test modules
        let key_pair = KeyPair::generate(&mut rand::rng());
        let mut peers = HashMap::new();,
        // Create a proper PeerAddr entry for testing
        peers.insert(0, crate::message::network::PeerAddr {
            node_id: 0,
            address: "127.0.0.1:8000".to_string(),fetyEngine::new()));
        }); metrics = Arc::new(MetricsCollector::new());
        let network_client = Arc::new(NetworkClient::new(node_id, peers));
        let timeout_manager = TimeoutManager::new(mple test setup
            Duration::from_secs(10), threshold for testing
            2.0_nodes = 4;
        );t (_aggregate_public_key, secret_keys) = BlsThresholdSigner::generate_keys(threshold, num_nodes)
        let state_machine: Arc<Mutex<dyn StateMachine>> = ");
            Arc::new(Mutex::new(TestStateMachine::new()));
        let mut public_keys = std::collections::HashMap::new();
        let f = 1; // Simplified for testingumerate() {
        let (message_sender, message_receiver) = tokio::sync::mpsc::channel(100);
        }
        // Initialize leader election with minimal nodes
        let nodes = vec![0, 1, 2, 3];Signer::new(
        let leader_election = LeaderElection::new(0, &nodes);
            threshold,
        // Initialize test modulesd as usize).unwrap_or(&secret_keys[0]).clone(),
        let pacemaker = Arc::new(Mutex::new(Pacemaker::new(
            Duration::from_millis(1000),hreshold signer");
            2.0,
        )));threshold_signer = Arc::new(Mutex::new(ThresholdSignatureManager::new(bls_signer)));

        let safety_engine = Arc::new(Mutex::new(SafetyEngine::new()));
        let metrics = Arc::new(MetricsCollector::new());

        // Initialize threshold signatures with simple test setup
        let threshold = 3; // Simple threshold for testing::SynchronyParameters::default();
        let num_nodes = 4;ctor = Arc::new(ProductionSynchronyDetector::new(node_id, synchrony_params));
        let (_aggregate_public_key, secret_keys) = BlsThresholdSigner::generate_keys(threshold, num_nodes)
            .expect("Failed to generate BLS threshold keys");
        let config = HotStuffConfig {
        let mut public_keys = std::collections::HashMap::new();
        for (i, sk) in secret_keys.iter().enumerate() {
            public_keys.insert(i as u64, sk.public_key());
        }       timeout_multiplier: 2.0,
                view_change_timeout_ms: 5000,
        let bls_signer = BlsThresholdSigner::new(
            node_id,batch_size: 50,
            threshold,timeout_ms: 100,
            secret_keys.get(node_id as usize).unwrap_or(&secret_keys[0]).clone(),
            public_keys,e_fault_tolerance: 1,
        ).expect("Failed to create BLS threshold signer");
                pipeline_depth: 3,
        let threshold_signer = Arc::new(Mutex::new(ThresholdSignatureManager::new(bls_signer)));
                optimistic_threshold: 0.8,
        // Initialize viewdetection_window: 10,
        let initial_view = View::new(0, nodes[0]); 
                latency_variance_threshold_ms: 100,
        // Initialize synchrony detector
        let synchrony_params = crate::consensus::synchrony::SynchronyParameters::default();
        let synchrony_detector = Arc::new(ProductionSynchronyDetector::new(node_id, synchrony_params));
        };
        // Create minimal config for testing
        let config = HotStuffConfig {nterface> = Arc::new(LegacyNetworkAdapter::new(network_client));
            consensus: crate::config::ConsensusConfig {
                optimistic_mode: true,
                base_timeout_ms: 1000,
                timeout_multiplier: 2.0,
                view_change_timeout_ms: 5000,
                max_transactions_per_block: 100,
                max_batch_size: 50,
                batch_timeout_ms: 100,
                max_block_size: 1024 * 1024, // 1MB
                byzantine_fault_tolerance: 1,
                enable_pipelining: true,
                pipeline_depth: 3,,
                fast_path_timeout_ms: 500,tate::default()),
                optimistic_threshold: 0.8,al_view),
                synchrony_detection_window: 10,
                max_network_delay_ms: 1000,
                latency_variance_threshold_ms: 100,
                max_view_changes: 10,
            },st_path_enabled: config.consensus.optimistic_mode,
            ..Default::default()ock::new(leader_election),
        };  view_change_timeout: Duration::from_millis(config.consensus.view_change_timeout_ms),
            threshold_signer,
        let network: Arc<dyn NetworkInterface> = Arc::new(LegacyNetworkAdapter::new(network_client));
                let tx_pool_config = crate::consensus::transaction_pool::TxPoolConfig {
        Arc::new(Self {_pool_size: config.consensus.max_transactions_per_block * 100,
            node_id,max_batch_size: config.consensus.max_batch_size,
            key_pair,atch_timeout: Duration::from_millis(config.consensus.batch_timeout_ms),
            network,..Default::default()
            block_store,
            timeout_manager,ductionTxPool::new(tx_pool_config))
            pacemaker,
            safety_engine,: config.consensus.max_batch_size,
            state_machine, Duration::from_millis(config.consensus.batch_timeout_ms),
            metrics,sender,
            config: config.clone(),::new(Some(message_receiver)),
            chain_state: Mutex::new(ChainState::default()),
            current_view: Mutex::new(initial_view),
            pipeline: DashMap::new(),
            votes: DashMap::new(),
            timeouts: DashMap::new(),
            synchrony_detector,lf, msg: ConsensusMsg) -> Result<(), HotStuffError> {
            fast_path_enabled: config.consensus.optimistic_mode,
            leader_election: RwLock::new(leader_election),e_proposal(proposal).await,
            view_change_timeout: Duration::from_millis(config.consensus.view_change_timeout_ms),
            threshold_signer,eout(timeout) => self.handle_timeout(timeout).await,
            transaction_pool: {ew(new_view) => self.handle_new_view(new_view).await,
                let tx_pool_config = crate::consensus::transaction_pool::TxPoolConfig {t).await,
                    max_pool_size: config.consensus.max_transactions_per_block * 100,
                    max_batch_size: config.consensus.max_batch_size,
                    batch_timeout: Duration::from_millis(config.consensus.batch_timeout_ms),
                    ..Default::default()from other nodes
                };e_fast_commit(&self, fast_commit: crate::message::consensus::FastCommit) -> Result<(), HotStuffError> {
                Arc::new(ProductionTxPool::new(tx_pool_config))
            },eceived fast commit notification for block {} at height {} from node {}",
            max_batch_size: config.consensus.max_batch_size,
            batch_timeout: Duration::from_millis(config.consensus.batch_timeout_ms),
            message_sender,
            message_receiver: Mutex::new(Some(message_receiver)),
            num_nodes: 4,
            f: 1, // Number of faulty nodes tolerance (n = 3f + 1)
}
    // Optimistic responsiveness - HotStuff-2 key innovation
    responsiveness_mode: ResponsivenessMode::Synchronous,
    optimistic_decision: Mutex::new(OptimisticDecision {
        use_optimistic_path: false,
        confidence_score: 0.0,ew.lock().await;
        network_delay_estimate: Duration::from_millis(100),for now
        last_updated: Instant::now(),
    }),p(view);
    view_change_cert: Mutex::new(None),r.start_timeout(height, round).await {
        error!("Failed to start initial timeout: {}", e);
    // Advanced HotStuff-2 features
    prepare_phase_enabled: false,
    fast_commit_threshold: 0.9, // Default 90% threshold for fast commit
}      if let Err(e) = this.handle_message(msg).await {
    }               error!("Error handling message: {}", e);
                }
    pub fn start(self: &Arc<Self>) {
        let this = Arc::clone(self);
    }
        tokio::spawn(async move {
            info!("Starting HotStuff-2 protocol for node {}", this.node_id);
        self.message_sender.clone()
            // Take the receiver out of the mutex
            let mut receiver_opt = this.message_receiver.lock().await;
            let mut receiver = receiver_opt.take().expect("HotStuff2 already started");
    pub async fn get_performance_statistics(&self) -> Result<PerformanceStatistics, HotStuffError> {
            // Start the initial timeout();
            let view = this.current_view.lock().await;t;
            let height = view.number; // Use view number as height for now
            let round = view.number;tatistics {
            drop(view);ght: chain_state.high_qc.as_ref().map_or(0, |qc| qc.height),
            if let Err(e) = this.timeout_manager.start_timeout(height, round).await {
                error!("Failed to start initial timeout: {}", e);ng_count().await,
            }s_synchronous: self.synchrony_detector.is_network_synchronous().await,
            fast_path_enabled: self.fast_path_enabled, // Use the actual fast path setting
            pipeline_stages: self.pipeline.len(), // Actual pipeline stages count
            last_commit_time: std::time::SystemTime::now(),
            throughput_tps: 0.0,public accessor for tests)
            latency_ms: 0.0,ector(&self) -> &Arc<ProductionSynchronyDetector> {
            network_conditions: NetworkConditions {
                is_synchronous: self.synchrony_detector.is_network_synchronous().await,
                confidence: 0.95,
                estimated_delay_ms: 100,laceholder for now)
            },fn process_pipeline_concurrent(&self) -> Result<(), HotStuffError> {
        }; Placeholder implementation
        Ok(())
        // Update with actual metrics if available
        if let Ok(metric_stats) = metrics.get_statistics().await {
            stats.throughput_tps = metric_stats.throughput_tps;
            stats.latency_ms = metric_stats.latency_ms;Result<(), HotStuffError> {
        }/ Placeholder implementation
        Ok(())
        Ok(stats)
    }
    /// Detect and handle Byzantine behavior (placeholder for now)
    /// Get node ID (public accessor for tests)havior(&self) -> Result<(), HotStuffError> {
    pub fn get_node_id(&self) -> u64 {
        self.node_id
    }

    /// Get synchrony detector (public accessor for tests)
    pub fn get_synchrony_detector(&self) -> &Arc<ProductionSynchronyDetector> {
        &self.synchrony_detector)
    }

    /// Process pipeline concurrently (placeholder for now)
    pub async fn process_pipeline_concurrent(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation
        Ok(())
    }

    /// Adaptive timeout management (placeholder for now)
    pub async fn adaptive_timeout_management(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation
        Ok(())
    }
}
    /// Detect and handle Byzantine behavior (placeholder for now)
    pub async fn detect_and_handle_byzantine_behavior(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation
        Ok(())
    }
    /// Health checkonsensus::state_machine::TestStateMachine;
    pub async fn health_check(&self) -> Result<String, HotStuffError> {
        Ok("healthy".to_string())enerate(&mut rand::rng());
    }   let mut peers = HashMap::new();
        // Create a proper PeerAddr entry for testing
    /// Shutdown the noderate::message::network::PeerAddr {
    pub async fn shutdown(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation.to_string(),
        Ok(())
    }   let network_client = Arc::new(NetworkClient::new(node_id, peers));
        let timeout_manager = TimeoutManager::new(
    /// Recover from failureecs(10), 
    pub async fn recover_from_failure(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation
        Ok(())ate_machine: Arc<Mutex<dyn StateMachine>> = 
    }       Arc::new(Mutex::new(TestStateMachine::new()));
}
        let f = 1; // Simplified for testing
impl<B: BlockStore + ?Sized + 'static> HotStuff2<B> {o::sync::mpsc::channel(100);
    #[cfg(any(test, feature = "byzantine"))]
    /// Create a simplified HotStuff2 instance for testing
    pub fn new_for_testing(node_id: u64, block_store: Arc<B>) -> Arc<Self> {
        use std::collections::HashMap;ection::new(0, &nodes);
        use crate::consensus::state_machine::TestStateMachine;
        // Initialize test modules
        let key_pair = KeyPair::generate(&mut rand::rng());
        let mut peers = HashMap::new();,
        // Create a proper PeerAddr entry for testing
        peers.insert(0, crate::message::network::PeerAddr {
            node_id: 0,
            address: "127.0.0.1:8000".to_string(),fetyEngine::new()));
        }); metrics = Arc::new(MetricsCollector::new());
        let network_client = Arc::new(NetworkClient::new(node_id, peers));
        let timeout_manager = TimeoutManager::new(mple test setup
            Duration::from_secs(10), threshold for testing
            2.0_nodes = 4;
        );t (_aggregate_public_key, secret_keys) = BlsThresholdSigner::generate_keys(threshold, num_nodes)
        let state_machine: Arc<Mutex<dyn StateMachine>> = ");
            Arc::new(Mutex::new(TestStateMachine::new()));
        let mut public_keys = std::collections::HashMap::new();
        let f = 1; // Simplified for testingumerate() {
        let (message_sender, message_receiver) = tokio::sync::mpsc::channel(100);
        }
        // Initialize leader election with minimal nodes
        let nodes = vec![0, 1, 2, 3];Signer::new(
        let leader_election = LeaderElection::new(0, &nodes);
            threshold,
        // Initialize test modulesd as usize).unwrap_or(&secret_keys[0]).clone(),
        let pacemaker = Arc::new(Mutex::new(Pacemaker::new(
            Duration::from_millis(1000),hreshold signer");
            2.0,
        )));threshold_signer = Arc::new(Mutex::new(ThresholdSignatureManager::new(bls_signer)));

        let safety_engine = Arc::new(Mutex::new(SafetyEngine::new()));
        let metrics = Arc::new(MetricsCollector::new());

        // Initialize threshold signatures with simple test setup
        let threshold = 3; // Simple threshold for testing::SynchronyParameters::default();
        let num_nodes = 4;ctor = Arc::new(ProductionSynchronyDetector::new(node_id, synchrony_params));
        let (_aggregate_public_key, secret_keys) = BlsThresholdSigner::generate_keys(threshold, num_nodes)
            .expect("Failed to generate BLS threshold keys");
        let config = HotStuffConfig {
        let mut public_keys = std::collections::HashMap::new();
        for (i, sk) in secret_keys.iter().enumerate() {
            public_keys.insert(i as u64, sk.public_key());
        }       timeout_multiplier: 2.0,
                view_change_timeout_ms: 5000,
        let bls_signer = BlsThresholdSigner::new(
            node_id,batch_size: 50,
            threshold,timeout_ms: 100,
            secret_keys.get(node_id as usize).unwrap_or(&secret_keys[0]).clone(),
            public_keys,e_fault_tolerance: 1,
        ).expect("Failed to create BLS threshold signer");
                pipeline_depth: 3,
        let threshold_signer = Arc::new(Mutex::new(ThresholdSignatureManager::new(bls_signer)));
                optimistic_threshold: 0.8,
        // Initialize viewdetection_window: 10,
        let initial_view = View::new(0, nodes[0]); 
                latency_variance_threshold_ms: 100,
        // Initialize synchrony detector
        let synchrony_params = crate::consensus::synchrony::SynchronyParameters::default();
        let synchrony_detector = Arc::new(ProductionSynchronyDetector::new(node_id, synchrony_params));
        };
        // Create minimal config for testing
        let config = HotStuffConfig {nterface> = Arc::new(LegacyNetworkAdapter::new(network_client));
            consensus: crate::config::ConsensusConfig {
                optimistic_mode: true,
                base_timeout_ms: 1000,
                timeout_multiplier: 2.0,
                view_change_timeout_ms: 5000,
                max_transactions_per_block: 100,
                max_batch_size: 50,
                batch_timeout_ms: 100,
                max_block_size: 1024 * 1024, // 1MB
                byzantine_fault_tolerance: 1,
                enable_pipelining: true,
                pipeline_depth: 3,,
                fast_path_timeout_ms: 500,tate::default()),
                optimistic_threshold: 0.8,al_view),
                synchrony_detection_window: 10,
                max_network_delay_ms: 1000,
                latency_variance_threshold_ms: 100,
                max_view_changes: 10,
            },st_path_enabled: config.consensus.optimistic_mode,
            ..Default::default()ock::new(leader_election),
        };  view_change_timeout: Duration::from_millis(config.consensus.view_change_timeout_ms),
            threshold_signer,
        let network: Arc<dyn NetworkInterface> = Arc::new(LegacyNetworkAdapter::new(network_client));
                let tx_pool_config = crate::consensus::transaction_pool::TxPoolConfig {
        Arc::new(Self {_pool_size: config.consensus.max_transactions_per_block * 100,
            node_id,max_batch_size: config.consensus.max_batch_size,
            key_pair,atch_timeout: Duration::from_millis(config.consensus.batch_timeout_ms),
            network,..Default::default()
            block_store,
            timeout_manager,ductionTxPool::new(tx_pool_config))
            pacemaker,
            safety_engine,: config.consensus.max_batch_size,
            state_machine, Duration::from_millis(config.consensus.batch_timeout_ms),
            metrics,sender,
            config: config.clone(),::new(Some(message_receiver)),
            chain_state: Mutex::new(ChainState::default()),
            current_view: Mutex::new(initial_view),
            pipeline: DashMap::new(),
            votes: DashMap::new(),
            timeouts: DashMap::new(),
            synchrony_detector,lf, msg: ConsensusMsg) -> Result<(), HotStuffError> {
            fast_path_enabled: config.consensus.optimistic_mode,
            leader_election: RwLock::new(leader_election),e_proposal(proposal).await,
            view_change_timeout: Duration::from_millis(config.consensus.view_change_timeout_ms),
            threshold_signer,eout(timeout) => self.handle_timeout(timeout).await,
            transaction_pool: {ew(new_view) => self.handle_new_view(new_view).await,
                let tx_pool_config = crate::consensus::transaction_pool::TxPoolConfig {t).await,
                    max_pool_size: config.consensus.max_transactions_per_block * 100,
                    max_batch_size: config.consensus.max_batch_size,
                    batch_timeout: Duration::from_millis(config.consensus.batch_timeout_ms),
                    ..Default::default()from other nodes
                };e_fast_commit(&self, fast_commit: crate::message::consensus::FastCommit) -> Result<(), HotStuffError> {
                Arc::new(ProductionTxPool::new(tx_pool_config))
            },eceived fast commit notification for block {} at height {} from node {}",
            max_batch_size: config.consensus.max_batch_size,
            batch_timeout: Duration::from_millis(config.consensus.batch_timeout_ms),
            message_sender,
            message_receiver: Mutex::new(Some(message_receiver)),
            num_nodes: 4,
            f: 1, // Number of faulty nodes tolerance (n = 3f + 1)
}
    // Optimistic responsiveness - HotStuff-2 key innovation
    responsiveness_mode: ResponsivenessMode::Synchronous,
    optimistic_decision: Mutex::new(OptimisticDecision {
        use_optimistic_path: false,
        confidence_score: 0.0,ew.lock().await;
        network_delay_estimate: Duration::from_millis(100),for now
        last_updated: Instant::now(),
    }),p(view);
    view_change_cert: Mutex::new(None),r.start_timeout(height, round).await {
        error!("Failed to start initial timeout: {}", e);
    // Advanced HotStuff-2 features
    prepare_phase_enabled: false,
    fast_commit_threshold: 0.9, // Default 90% threshold for fast commit
}      if let Err(e) = this.handle_message(msg).await {
    }               error!("Error handling message: {}", e);
                }
    pub fn start(self: &Arc<Self>) {
        let this = Arc::clone(self);
    }
        tokio::spawn(async move {
            info!("Starting HotStuff-2 protocol for node {}", this.node_id);
        self.message_sender.clone()
            // Take the receiver out of the mutex
            let mut receiver_opt = this.message_receiver.lock().await;
            let mut receiver = receiver_opt.take().expect("HotStuff2 already started");
    pub async fn get_performance_statistics(&self) -> Result<PerformanceStatistics, HotStuffError> {
            // Start the initial timeout();
            let view = this.current_view.lock().await;t;
            let height = view.number; // Use view number as height for now
            let round = view.number;tatistics {
            drop(view);ght: chain_state.high_qc.as_ref().map_or(0, |qc| qc.height),
            if let Err(e) = this.timeout_manager.start_timeout(height, round).await {
                error!("Failed to start initial timeout: {}", e);ng_count().await,
            }s_synchronous: self.synchrony_detector.is_network_synchronous().await,
            fast_path_enabled: self.fast_path_enabled, // Use the actual fast path setting
            pipeline_stages: self.pipeline.len(), // Actual pipeline stages count
            last_commit_time: std::time::SystemTime::now(),
            throughput_tps: 0.0,public accessor for tests)
            latency_ms: 0.0,ector(&self) -> &Arc<ProductionSynchronyDetector> {
            network_conditions: NetworkConditions {
                is_synchronous: self.synchrony_detector.is_network_synchronous().await,
                confidence: 0.95,
                estimated_delay_ms: 100,laceholder for now)
            },fn process_pipeline_concurrent(&self) -> Result<(), HotStuffError> {
        }; Placeholder implementation
        Ok(())
        // Update with actual metrics if available
        if let Ok(metric_stats) = metrics.get_statistics().await {
            stats.throughput_tps = metric_stats.throughput_tps;
            stats.latency_ms = metric_stats.latency_ms;Result<(), HotStuffError> {
        }/ Placeholder implementation
        Ok(())
        Ok(stats)
    }
    /// Detect and handle Byzantine behavior (placeholder for now)
    /// Get node ID (public accessor for tests)havior(&self) -> Result<(), HotStuffError> {
    pub fn get_node_id(&self) -> u64 {
        self.node_id
    }

    /// Get synchrony detector (public accessor for tests)
    pub fn get_synchrony_detector(&self) -> &Arc<ProductionSynchronyDetector> {
        &self.synchrony_detector)
    }

    /// Process pipeline concurrently (placeholder for now)
    pub async fn process_pipeline_concurrent(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation
        Ok(())
    }

    /// Adaptive timeout management (placeholder for now)
    pub async fn adaptive_timeout_management(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation
        Ok(())
    }
}
    /// Detect and handle Byzantine behavior (placeholder for now)
    pub async fn detect_and_handle_byzantine_behavior(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation
        Ok(())
    }
    /// Health checkonsensus::state_machine::TestStateMachine;
    pub async fn health_check(&self) -> Result<String, HotStuffError> {
        Ok("healthy".to_string())enerate(&mut rand::rng());
    }   let mut peers = HashMap::new();
        // Create a proper PeerAddr entry for testing
    /// Shutdown the noderate::message::network::PeerAddr {
    pub async fn shutdown(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation.to_string(),
        Ok(())
    }   let network_client = Arc::new(NetworkClient::new(node_id, peers));
        let timeout_manager = TimeoutManager::new(
    /// Recover from failureecs(10), 
    pub async fn recover_from_failure(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation
        Ok(())ate_machine: Arc<Mutex<dyn StateMachine>> = 
    }       Arc::new(Mutex::new(TestStateMachine::new()));
}
        let f = 1; // Simplified for testing
impl<B: BlockStore + ?Sized + 'static> HotStuff2<B> {o::sync::mpsc::channel(100);
    #[cfg(any(test, feature = "byzantine"))]
    /// Create a simplified HotStuff2 instance for testing
    pub fn new_for_testing(node_id: u64, block_store: Arc<B>) -> Arc<Self> {
        use std::collections::HashMap;ection::new(0, &nodes);
        use crate::consensus::state_machine::TestStateMachine;
        // Initialize test modules
        let key_pair = KeyPair::generate(&mut rand::rng());
        let mut peers = HashMap::new();,
        // Create a proper PeerAddr entry for testing
        peers.insert(0, crate::message::network::PeerAddr {
            node_id: 0,
            address: "127.0.0.1:8000".to_string(),fetyEngine::new()));
        }); metrics = Arc::new(MetricsCollector::new());
        let network_client = Arc::new(NetworkClient::new(node_id, peers));
        let timeout_manager = TimeoutManager::new(mple test setup
            Duration::from_secs(10), threshold for testing
            2.0_nodes = 4;
        );t (_aggregate_public_key, secret_keys) = BlsThresholdSigner::generate_keys(threshold, num_nodes)
        let state_machine: Arc<Mutex<dyn StateMachine>> = ");
            Arc::new(Mutex::new(TestStateMachine::new()));
        let mut public_keys = std::collections::HashMap::new();
        let f = 1; // Simplified for testingumerate() {
        let (message_sender, message_receiver) = tokio::sync::mpsc::channel(100);
        }
        // Initialize leader election with minimal nodes
        let nodes = vec![0, 1, 2, 3];Signer::new(
        let leader_election = LeaderElection::new(0, &nodes);
            threshold,
        // Initialize test modulesd as usize).unwrap_or(&secret_keys[0]).clone(),
        let pacemaker = Arc::new(Mutex::new(Pacemaker::new(
            Duration::from_millis(1000),hreshold signer");
            2.0,
        )));threshold_signer = Arc::new(Mutex::new(ThresholdSignatureManager::new(bls_signer)));

        let safety_engine = Arc::new(Mutex::new(SafetyEngine::new()));
        let metrics = Arc::new(MetricsCollector::new());

        // Initialize threshold signatures with simple test setup
        let threshold = 3; // Simple threshold for testing::SynchronyParameters::default();
        let num_nodes = 4;ctor = Arc::new(ProductionSynchronyDetector::new(node_id, synchrony_params));
        let (_aggregate_public_key, secret_keys) = BlsThresholdSigner::generate_keys(threshold, num_nodes)
            .expect("Failed to generate BLS threshold keys");
        let config = HotStuffConfig {
        let mut public_keys = std::collections::HashMap::new();
        for (i, sk) in secret_keys.iter().enumerate() {
            public_keys.insert(i as u64, sk.public_key());
        }       timeout_multiplier: 2.0,
                view_change_timeout_ms: 5000,
        let bls_signer = BlsThresholdSigner::new(
            node_id,batch_size: 50,
            threshold,timeout_ms: 100,
            secret_keys.get(node_id as usize).unwrap_or(&secret_keys[0]).clone(),
            public_keys,e_fault_tolerance: 1,
        ).expect("Failed to create BLS threshold signer");
                pipeline_depth: 3,
        let threshold_signer = Arc::new(Mutex::new(ThresholdSignatureManager::new(bls_signer)));
                optimistic_threshold: 0.8,
        // Initialize viewdetection_window: 10,
        let initial_view = View::new(0, nodes[0]); 
                latency_variance_threshold_ms: 100,
        // Initialize synchrony detector
        let synchrony_params = crate::consensus::synchrony::SynchronyParameters::default();
        let synchrony_detector = Arc::new(ProductionSynchronyDetector::new(node_id, synchrony_params));
        };
        // Create minimal config for testing
        let config = HotStuffConfig {nterface> = Arc::new(LegacyNetworkAdapter::new(network_client));
            consensus: crate::config::ConsensusConfig {
                optimistic_mode: true,
                base_timeout_ms: 1000,
                timeout_multiplier: 2.0,
                view_change_timeout_ms: 5000,
                max_transactions_per_block: 100,
                max_batch_size: 50,
                batch_timeout_ms: 100,
                max_block_size: 1024 * 1024, // 1MB
                byzantine_fault_tolerance: 1,
                enable_pipelining: true,
                pipeline_depth: 3,,
                fast_path_timeout_ms: 500,tate::default()),
                optimistic_threshold: 0.8,al_view),
                synchrony_detection_window: 10,
                max_network_delay_ms: 1000,
                latency_variance_threshold_ms: 100,
                max_view_changes: 10,
            },st_path_enabled: config.consensus.optimistic_mode,
            ..Default::default()ock::new(leader_election),
        };  view_change_timeout: Duration::from_millis(config.consensus.view_change_timeout_ms),
            threshold_signer,
        let network: Arc<dyn NetworkInterface> = Arc::new(LegacyNetworkAdapter::new(network_client));
                let tx_pool_config = crate::consensus::transaction_pool::TxPoolConfig {
        Arc::new(Self {_pool_size: config.consensus.max_transactions_per_block * 100,
            node_id,max_batch_size: config.consensus.max_batch_size,
            key_pair,atch_timeout: Duration::from_millis(config.consensus.batch_timeout_ms),
            network,..Default::default()
            block_store,
            timeout_manager,ductionTxPool::new(tx_pool_config))
            pacemaker,
            safety_engine,: config.consensus.max_batch_size,
            state_machine, Duration::from_millis(config.consensus.batch_timeout_ms),
            metrics,sender,
            config: config.clone(),::new(Some(message_receiver)),
            chain_state: Mutex::new(ChainState::default()),
            current_view: Mutex::new(initial_view),
            pipeline: DashMap::new(),
            votes: DashMap::new(),
            timeouts: DashMap::new(),
            synchrony_detector,lf, msg: ConsensusMsg) -> Result<(), HotStuffError> {
            fast_path_enabled: config.consensus.optimistic_mode,
            leader_election: RwLock::new(leader_election),e_proposal(proposal).await,
            view_change_timeout: Duration::from_millis(config.consensus.view_change_timeout_ms),
            threshold_signer,eout(timeout) => self.handle_timeout(timeout).await,
            transaction_pool: {ew(new_view) => self.handle_new_view(new_view).await,
                let tx_pool_config = crate::consensus::transaction_pool::TxPoolConfig {t).await,
                    max_pool_size: config.consensus.max_transactions_per_block * 100,
                    max_batch_size: config.consensus.max_batch_size,
                    batch_timeout: Duration::from_millis(config.consensus.batch_timeout_ms),
                    ..Default::default()from other nodes
                };e_fast_commit(&self, fast_commit: crate::message::consensus::FastCommit) -> Result<(), HotStuffError> {
                Arc::new(ProductionTxPool::new(tx_pool_config))
            },eceived fast commit notification for block {} at height {} from node {}",
            max_batch_size: config.consensus.max_batch_size,
            batch_timeout: Duration::from_millis(config.consensus.batch_timeout_ms),
            message_sender,
            message_receiver: Mutex::new(Some(message_receiver)),
            num_nodes: 4,
            f: 1, // Number of faulty nodes tolerance (n = 3f + 1)
}
    // Optimistic responsiveness - HotStuff-2 key innovation
    responsiveness_mode: ResponsivenessMode::Synchronous,
    optimistic_decision: Mutex::new(OptimisticDecision {
        use_optimistic_path: false,
        confidence_score: 0.0,ew.lock().await;
        network_delay_estimate: Duration::from_millis(100),for now
        last_updated: Instant::now(),
    }),p(view);
    view_change_cert: Mutex::new(None),r.start_timeout(height, round).await {
        error!("Failed to start initial timeout: {}", e);
    // Advanced HotStuff-2 features
    prepare_phase_enabled: false,
    fast_commit_threshold: 0.9, // Default 90% threshold for fast commit
}      if let Err(e) = this.handle_message(msg).await {
    }               error!("Error handling message: {}", e);
                }
    pub fn start(self: &Arc<Self>) {
        let this = Arc::clone(self);
    }
        tokio::spawn(async move {
            info!("Starting HotStuff-2 protocol for node {}", this.node_id);
        self.message_sender.clone()
            // Take the receiver out of the mutex
            let mut receiver_opt = this.message_receiver.lock().await;
            let mut receiver = receiver_opt.take().expect("HotStuff2 already started");
    pub async fn get_performance_statistics(&self) -> Result<PerformanceStatistics, HotStuffError> {
            // Start the initial timeout();
            let view = this.current_view.lock().await;t;
            let height = view.number; // Use view number as height for now
            let round = view.number;tatistics {
            drop(view);ght: chain_state.high_qc.as_ref().map_or(0, |qc| qc.height),
            if let Err(e) = this.timeout_manager.start_timeout(height, round).await {
                error!("Failed to start initial timeout: {}", e);ng_count().await,
            }s_synchronous: self.synchrony_detector.is_network_synchronous().await,
            fast_path_enabled: self.fast_path_enabled, // Use the actual fast path setting
            pipeline_stages: self.pipeline.len(), // Actual pipeline stages count
            last_commit_time: std::time::SystemTime::now(),
            throughput_tps: 0.0,public accessor for tests)
            latency_ms: 0.0,ector(&self) -> &Arc<ProductionSynchronyDetector> {
            network_conditions: NetworkConditions {
                is_synchronous: self.synchrony_detector.is_network_synchronous().await,
                confidence: 0.95,
                estimated_delay_ms: 100,laceholder for now)
            },fn process_pipeline_concurrent(&self) -> Result<(), HotStuffError> {
        }; Placeholder implementation
        Ok(())
        // Update with actual metrics if available
        if let Ok(metric_stats) = metrics.get_statistics().await {
            stats.throughput_tps = metric_stats.throughput_tps;
            stats.latency_ms = metric_stats.latency_ms;Result<(), HotStuffError> {
        }/ Placeholder implementation
        Ok(())
        Ok(stats)
    }
    /// Detect and handle Byzantine behavior (placeholder for now)
    /// Get node ID (public accessor for tests)havior(&self) -> Result<(), HotStuffError> {
    pub fn get_node_id(&self) -> u64 {
        self.node_id
    }

    /// Get synchrony detector (public accessor for tests)
    pub fn get_synchrony_detector(&self) -> &Arc<ProductionSynchronyDetector> {
        &self.synchrony_detector)
    }

    /// Process pipeline concurrently (placeholder for now)
    pub async fn process_pipeline_concurrent(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation
        Ok(())
    }

    /// Adaptive timeout management (placeholder for now)
    pub async fn adaptive_timeout_management(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation
        Ok(())
    }
}
    /// Detect and handle Byzantine behavior (placeholder for now)
    pub async fn detect_and_handle_byzantine_behavior(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation
        Ok(())
    }
}
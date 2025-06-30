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

/// State checkpoint metadata for recovery operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateCheckpointMetadata {
    pub height: u64,
    pub state_hash: Hash,
    pub timestamp: std::time::SystemTime,
    pub node_id: u64,
}

/// Synchrony detection for optimistic responsiveness
pub struct HotStuff2<B: BlockStore + ?Sized + 'static> {
    node_id: u64,
    key_pair: KeyPair,
    network: Arc<dyn NetworkInterface>,
    block_store: Arc<B>,
    timeout_manager: Arc<TimeoutManager>,
    pacemaker: Arc<Mutex<Pacemaker>>,
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
    votes: DashMap<Hash, Vec<Vote>>,
    timeouts: DashMap<u64, Vec<Timeout>>, // view -> Timeouts
    
    // Optimistic responsiveness for fast path
    synchrony_detector: Arc<ProductionSynchronyDetector>,
    fast_path_enabled: bool,
    
    // Leader election and view change
    leader_election: RwLock<LeaderElection>,
    view_change_timeout: Duration,
    
    // Threshold signatures for efficient aggregation
    threshold_signer: Arc<Mutex<ThresholdSignatureManager>>,
    
    // Transaction batching for high throughput
    transaction_pool: Arc<ProductionTxPool>,
    max_batch_size: usize,
    batch_timeout: Duration,
    
    message_sender: mpsc::Sender<ConsensusMsg>,
    message_receiver: Mutex<Option<mpsc::Receiver<ConsensusMsg>>>,
    num_nodes: u64,
    f: u64, // Number of faulty nodes tolerance (n = 3f + 1)
}

struct LeaderElection {
    epoch: u64,
    leader_rotation: Vec<u64>,
}

impl LeaderElection {
    fn new(epoch: u64, nodes: &[u64]) -> Self {
        let mut leader_rotation = nodes.to_vec();
        // Simple rotation based on epoch
        let len = leader_rotation.len() as u64;
        leader_rotation.rotate_left((epoch % len) as usize);

        Self {
            epoch,
            leader_rotation,
        }
    }

    fn get_leader(&self, round: u64) -> u64 {
        let index = (round % self.leader_rotation.len() as u64) as usize;
        self.leader_rotation[index]
    }

    fn next_epoch(&mut self, nodes: &[u64]) {
        self.epoch += 1;
        let mut leader_rotation = nodes.to_vec();
        let len = leader_rotation.len() as u64;
        leader_rotation.rotate_left((self.epoch % len) as usize);
        self.leader_rotation = leader_rotation;
    }
    
    fn get_leader_for_view(&self, view: u64) -> u64 {
        self.get_leader(view)
    }
}

impl<B: BlockStore + ?Sized + 'static> HotStuff2<B> {
    pub fn new(
        node_id: u64,
        key_pair: KeyPair,
        network_client: Arc<NetworkClient>,
        block_store: Arc<B>,
        timeout_manager: Arc<TimeoutManager>,
        num_nodes: u64,
        config: HotStuffConfig,
        state_machine: Arc<Mutex<dyn StateMachine>>,
    ) -> Arc<Self> {
        let network: Arc<dyn NetworkInterface> = Arc::new(LegacyNetworkAdapter::new(network_client));
        Self::new_with_network(node_id, key_pair, network, block_store, timeout_manager, num_nodes, config, state_machine)
    }

    pub fn new_with_p2p(
        node_id: u64,
        key_pair: KeyPair,
        p2p_network: Arc<P2PNetwork>,
        block_store: Arc<B>,
        timeout_manager: Arc<TimeoutManager>,
        num_nodes: u64,
        config: HotStuffConfig,
        state_machine: Arc<Mutex<dyn StateMachine>>,
    ) -> Arc<Self> {
        let network: Arc<dyn NetworkInterface> = Arc::new(P2PNetworkAdapter::new(p2p_network));
        Self::new_with_network(node_id, key_pair, network, block_store, timeout_manager, num_nodes, config, state_machine)
    }

    fn new_with_network(
        node_id: u64,
        key_pair: KeyPair,
        network: Arc<dyn NetworkInterface>,
        block_store: Arc<B>,
        timeout_manager: Arc<TimeoutManager>,
        num_nodes: u64,
        config: HotStuffConfig,
        state_machine: Arc<Mutex<dyn StateMachine>>,
    ) -> Arc<Self> {
        // Tolerate f faulty nodes where n = 3f + 1
        let f = (num_nodes - 1) / 3;
        let (message_sender, message_receiver) = mpsc::channel(100);
        // Initialize leader election with node IDs from 0 to num_nodes-1
        let nodes: Vec<_> = (0..num_nodes).collect();
        let leader_election = LeaderElection::new(0, &nodes);

        // Initialize new consensus modules
        let pacemaker = Arc::new(Mutex::new(Pacemaker::new(
            Duration::from_millis(config.consensus.base_timeout_ms),
            config.consensus.timeout_multiplier,
        )));

        let safety_engine = Arc::new(Mutex::new(SafetyEngine::new()));
        let metrics = Arc::new(MetricsCollector::new());

        // Initialize threshold signatures with BLS
        let threshold = (num_nodes * 2 / 3) + 1; // Byzantine threshold
        let (_aggregate_public_key, secret_keys) = BlsThresholdSigner::generate_keys(threshold as usize, num_nodes as usize)
            .expect("Failed to generate BLS threshold keys");
        
        // Create individual public keys for each node
        let mut public_keys = std::collections::HashMap::new();
        for (i, sk) in secret_keys.iter().enumerate() {
            public_keys.insert(i as u64, sk.public_key());
        }
        
        let bls_signer = BlsThresholdSigner::new(
            node_id,
            threshold as usize,
            secret_keys[node_id as usize].clone(),
            public_keys,
        ).expect("Failed to create BLS threshold signer");
        
        let threshold_signer = Arc::new(Mutex::new(ThresholdSignatureManager::new(bls_signer)));

        // Initialize view
        let initial_view = View::new(0, nodes[0]); // Start with first node as leader

        // Initialize production synchrony detector
        let synchrony_params = crate::consensus::synchrony::SynchronyParameters::default();
        let synchrony_detector = Arc::new(ProductionSynchronyDetector::new(node_id, synchrony_params));

        Arc::new(Self {
            node_id,
            key_pair,
            network,
            block_store,
            timeout_manager,
            pacemaker,
            safety_engine,
            state_machine,
            metrics,
            config: config.clone(),
            chain_state: Mutex::new(ChainState::default()),
            current_view: Mutex::new(initial_view),
            pipeline: DashMap::new(),
            votes: DashMap::new(),
            timeouts: DashMap::new(),
            synchrony_detector,
            fast_path_enabled: config.consensus.optimistic_mode,
            leader_election: RwLock::new(leader_election),
            view_change_timeout: Duration::from_millis(config.consensus.view_change_timeout_ms),
            threshold_signer,
            // Initialize production-grade transaction pool
            transaction_pool: {
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
            num_nodes,
            f,
        })
    }

    pub fn start(self: &Arc<Self>) {
        let this = Arc::clone(self);

        tokio::spawn(async move {
            info!("Starting HotStuff-2 protocol for node {}", this.node_id);

            // Take the receiver out of the mutex
            let mut receiver_opt = this.message_receiver.lock().await;
            let mut receiver = receiver_opt.take().expect("HotStuff2 already started");

            // Start the initial timeout
            let view = this.current_view.lock().await;
            let height = view.number; // Use view number as height for now
            let round = view.number;
            drop(view);
            if let Err(e) = this.timeout_manager.start_timeout(height, round).await {
                error!("Failed to start initial timeout: {}", e);
            }

            while let Some(msg) = receiver.recv().await {
                if let Err(e) = this.handle_message(msg).await {
                    error!("Error handling message: {}", e);
                }
            }
        });
    }

    pub fn get_message_sender(&self) -> mpsc::Sender<ConsensusMsg> {
        self.message_sender.clone()
    }

    /// Get performance statistics
    pub async fn get_performance_statistics(&self) -> Result<PerformanceStatistics, HotStuffError> {
        let metrics = self.metrics.clone();
        let chain_state = self.chain_state.lock().await;
        let current_view = self.current_view.lock().await;
        let mut stats = PerformanceStatistics {
            current_height: chain_state.high_qc.as_ref().map_or(0, |qc| qc.height),
            current_view: current_view.number,
            pending_transactions: self.transaction_pool.get_pending_count().await,
            is_synchronous: self.synchrony_detector.is_network_synchronous().await,
            fast_path_enabled: self.fast_path_enabled, // Use the actual fast path setting
            pipeline_stages: self.pipeline.len(), // Actual pipeline stages count
            last_commit_time: std::time::SystemTime::now(),
            throughput_tps: 0.0,
            latency_ms: 0.0,
            network_conditions: NetworkConditions {
                is_synchronous: self.synchrony_detector.is_network_synchronous().await,
                confidence: 0.95,
                estimated_delay_ms: 100,
            },
        };

        // Update with actual metrics if available
        if let Ok(metric_stats) = metrics.get_statistics().await {
            stats.throughput_tps = metric_stats.throughput_tps;
            stats.latency_ms = metric_stats.latency_ms;
        }

        Ok(stats)
    }

    /// Get node ID (public accessor for tests)
    pub fn get_node_id(&self) -> u64 {
        self.node_id
    }

    /// Get synchrony detector (public accessor for tests)
    pub fn get_synchrony_detector(&self) -> &Arc<ProductionSynchronyDetector> {
        &self.synchrony_detector
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

    /// Detect and handle Byzantine behavior (placeholder for now)
    pub async fn detect_and_handle_byzantine_behavior(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation
        Ok(())
    }

    /// Health check
    pub async fn health_check(&self) -> Result<String, HotStuffError> {
        Ok("healthy".to_string())
    }

    /// Shutdown the node
    pub async fn shutdown(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation
        Ok(())
    }

    /// Recover from failure
    pub async fn recover_from_failure(&self) -> Result<(), HotStuffError> {
        // Placeholder implementation
        Ok(())
    }
}

impl<B: BlockStore + ?Sized + 'static> HotStuff2<B> {
    #[cfg(any(test, feature = "byzantine"))]
    /// Create a simplified HotStuff2 instance for testing
    pub fn new_for_testing(node_id: u64, block_store: Arc<B>) -> Arc<Self> {
        use std::collections::HashMap;
        use crate::consensus::state_machine::TestStateMachine;
        
        let key_pair = KeyPair::generate(&mut rand::rng());
        let mut peers = HashMap::new();
        // Create a proper PeerAddr entry for testing
        peers.insert(0, crate::message::network::PeerAddr {
            node_id: 0,
            address: "127.0.0.1:8000".to_string(),
        });
        let network_client = Arc::new(NetworkClient::new(node_id, peers));
        let timeout_manager = TimeoutManager::new(
            Duration::from_secs(10), 
            2.0
        );
        let state_machine: Arc<Mutex<dyn StateMachine>> = 
            Arc::new(Mutex::new(TestStateMachine::new()));

        let f = 1; // Simplified for testing
        let (message_sender, message_receiver) = tokio::sync::mpsc::channel(100);
        
        // Initialize leader election with minimal nodes
        let nodes = vec![0, 1, 2, 3];
        let leader_election = LeaderElection::new(0, &nodes);

        // Initialize test modules
        let pacemaker = Arc::new(Mutex::new(Pacemaker::new(
            Duration::from_millis(1000),
            2.0,
        )));

        let safety_engine = Arc::new(Mutex::new(SafetyEngine::new()));
        let metrics = Arc::new(MetricsCollector::new());

        // Initialize threshold signatures with simple test setup
        let threshold = 3; // Simple threshold for testing
        let num_nodes = 4;
        let (_aggregate_public_key, secret_keys) = BlsThresholdSigner::generate_keys(threshold, num_nodes)
            .expect("Failed to generate BLS threshold keys");
        
        let mut public_keys = std::collections::HashMap::new();
        for (i, sk) in secret_keys.iter().enumerate() {
            public_keys.insert(i as u64, sk.public_key());
        }
        
        let bls_signer = BlsThresholdSigner::new(
            node_id,
            threshold,
            secret_keys.get(node_id as usize).unwrap_or(&secret_keys[0]).clone(),
            public_keys,
        ).expect("Failed to create BLS threshold signer");
        
        let threshold_signer = Arc::new(Mutex::new(ThresholdSignatureManager::new(bls_signer)));

        // Initialize view
        let initial_view = View::new(0, nodes[0]); 

        // Initialize synchrony detector
        let synchrony_params = crate::consensus::synchrony::SynchronyParameters::default();
        let synchrony_detector = Arc::new(ProductionSynchronyDetector::new(node_id, synchrony_params));

        // Create minimal config for testing
        let config = HotStuffConfig {
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
                pipeline_depth: 3,
                fast_path_timeout_ms: 500,
                optimistic_threshold: 0.8,
                synchrony_detection_window: 10,
                max_network_delay_ms: 1000,
                latency_variance_threshold_ms: 100,
                max_view_changes: 10,
            },
            ..Default::default()
        };

        let network: Arc<dyn NetworkInterface> = Arc::new(LegacyNetworkAdapter::new(network_client));

        Arc::new(Self {
            node_id,
            key_pair,
            network,
            block_store,
            timeout_manager,
            pacemaker,
            safety_engine,
            state_machine,
            metrics,
            config: config.clone(),
            chain_state: Mutex::new(ChainState::default()),
            current_view: Mutex::new(initial_view),
            pipeline: DashMap::new(),
            votes: DashMap::new(),
            timeouts: DashMap::new(),
            synchrony_detector,
            fast_path_enabled: config.consensus.optimistic_mode,
            leader_election: RwLock::new(leader_election),
            view_change_timeout: Duration::from_millis(config.consensus.view_change_timeout_ms),
            threshold_signer,
            transaction_pool: {
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
            f,
        })
    }

    async fn handle_message(&self, msg: ConsensusMsg) -> Result<(), HotStuffError> {
        match msg {
            ConsensusMsg::Proposal(proposal) => self.handle_proposal(proposal).await,
            ConsensusMsg::Vote(vote) => self.handle_vote(vote).await,
            ConsensusMsg::Timeout(timeout) => self.handle_timeout(timeout).await,
            ConsensusMsg::NewView(new_view) => self.handle_new_view(new_view).await,
            ConsensusMsg::FastCommit(fast_commit) => self.handle_fast_commit(fast_commit).await,
        }
    }

    /// Handle fast commit notification from other nodes
    async fn handle_fast_commit(&self, fast_commit: crate::message::consensus::FastCommit) -> Result<(), HotStuffError> {
        debug!(
            "Received fast commit notification for block {} at height {} from node {}",
            fast_commit.block_hash, fast_commit.height, fast_commit.node_id
        );
        
        // Verify we have this block
        if let Some(block) = self.block_store.get_block(&fast_commit.block_hash)? {
            // Update our committed height if this is newer
            let chain_state = self.chain_state.lock().await;
            if fast_commit.height > chain_state.committed_height {
                drop(chain_state);
                
                // Execute the block through state machine for consistency
                let new_state_hash = self.execute_block_with_state_transition(&block).await?;
                
                let mut chain_state = self.chain_state.lock().await;
                chain_state.committed_height = fast_commit.height;
                chain_state.b_exec = Some(fast_commit.block_hash);
                chain_state.last_committed_state = new_state_hash;
                drop(chain_state);
                
                info!("✅ Updated committed height to {} due to fast commit notification with state hash {}", 
                      fast_commit.height, new_state_hash);
            }
        } else {
            debug!("Received fast commit for unknown block {}", fast_commit.block_hash);
        }
        
        Ok(())
    }

    /// Handle proposal with integrated state validation
    async fn handle_proposal(&self, proposal: Proposal) -> Result<(), HotStuffError> {
        let block = &proposal.block;
        let view = self.current_view.lock().await;
        let current_view_num = view.number;
        let expected_leader = view.leader;
        drop(view);

        // Check if proposal is from the correct leader for current view
        if block.proposer_id != expected_leader {
            warn!(
                "Received proposal from {} but leader for view {} is {}",
                block.proposer_id, current_view_num, expected_leader
            );
            return Ok(());
        }

        // Verify block structure and state consistency before voting
        if let Err(e) = self.validate_block_for_execution(block).await {
            warn!("Block {} failed validation: {}", block.hash, e);
            return Ok(());
        }

        // Verify block chaining - block should extend high_qc
        let chain_state = self.chain_state.lock().await;
        let can_vote = self.verify_block_safety(block, &chain_state).await?;
        drop(chain_state);

        if !can_vote {
            warn!("Block {} failed safety verification", block.hash);
            return Ok(());
        }

        // Store the block
        self.block_store.put_block(block)?;

        // Create or update pipeline stage
        let mut stage = self.pipeline.entry(block.height)
            .or_insert_with(|| PipelineStage::new(block.height, current_view_num));
        stage.proposal = Some(proposal.clone());

        // Update safety state and vote
        let mut chain_state = self.chain_state.lock().await;
        chain_state.last_voted_round = current_view_num;
        drop(chain_state);

        // Vote for the block
        self.send_vote(block).await?;

        Ok(())
    }

    /// Enhanced safety verification for HotStuff-2
    async fn verify_block_safety(&self, block: &Block, chain_state: &ChainState) -> Result<bool, HotStuffError> {
        // Check view number
        if self.current_view.lock().await.number <= chain_state.last_voted_round {
            return Ok(false); // Already voted in this view or later
        }

        // Check if block extends our high_qc or locked block
        if let Some(high_qc) = &chain_state.high_qc {
            if block.parent_hash != high_qc.block_hash && block.height != high_qc.height + 1 {
                // Block doesn't extend high_qc
                if let Some(locked_hash) = &chain_state.b_lock {
                    // Check if block extends locked block
                    return Ok(self.extends_block(block, locked_hash).await?);
                }
                return Ok(false);
            }
        }

        // Additional safety checks
        if block.height <= chain_state.committed_height {
            return Ok(false); // Block is too old
        }

        Ok(true)
    }

    /// Check if block extends another block (enhanced version)
    async fn extends_block(&self, block: &Block, target_hash: &Hash) -> Result<bool, HotStuffError> {
        if block.parent_hash == *target_hash {
            return Ok(true);
        }
        
        // Check if block transitively extends target through chain
        let mut current_hash = block.parent_hash;
        let mut depth = 0;
        const MAX_DEPTH: usize = 100; // Prevent infinite loops
        
        while depth < MAX_DEPTH {
            if current_hash == *target_hash {
                return Ok(true);
            }
            
            if let Some(parent_block) = self.block_store.get_block(&current_hash)? {
                current_hash = parent_block.parent_hash;
                depth += 1;
            } else {
                break;
            }
        }
        
        Ok(false)
    }

    async fn send_vote(&self, block: &Block) -> Result<(), HotStuffError> {
        // Sign the block hash with traditional signature for compatibility
        let signature = self.key_pair.sign(block.hash().as_bytes())?;

        // Create BLS partial signature for efficient threshold aggregation
        let threshold_signer = self.threshold_signer.lock().await;
        let partial_signature = threshold_signer.sign_partial(block.hash().as_bytes())?;
        drop(threshold_signer);

        let vote = Vote {
            block_hash: block.hash(),
            height: block.height,
            view: self.current_view.lock().await.number,
            sender_id: self.node_id,
            signature,
            partial_signature: Some(partial_signature),
        };

        // Broadcast the vote
        let network_msg = crate::message::network::NetworkMsg::Consensus(ConsensusMsg::Vote(vote));
        self.network.broadcast_message(network_msg).await?;

        Ok(())
    }

    // HotStuff-2 safety rule: can only vote if block extends locked QC or has higher QC
    async fn safe_to_vote(&self, block: &Block, chain_state: &ChainState) -> Result<bool, HotStuffError> {
        // If we don't have a locked QC, we can vote for any valid block
        let locked_qc = match &chain_state.locked_qc {
            Some(qc) => qc,
            None => return Ok(true),
        };

        // Safety rule: can only vote for block that extends our locked QC
        // or if we have a higher QC than our locked QC
        if let Some(high_qc) = &chain_state.high_qc {
            if high_qc.height > locked_qc.height {
                return Ok(true);
            }
        }

        // Check if this block extends the locked QC
        self.extends_qc(block, locked_qc).await
    }

    async fn extends_qc(&self, block: &Block, qc: &QuorumCert) -> Result<bool, HotStuffError> {
        // Simple check: block should have height > qc.height
        // In a full implementation, we'd verify the full chain connection
        Ok(block.height > qc.height)
    }

    async fn handle_vote(&self, vote: Vote) -> Result<(), HotStuffError> {
        let view = self.current_view.lock().await;
        let current_view_num = view.number;
        let am_leader = view.leader == self.node_id;
        drop(view);

        // Only leader processes votes
        if !am_leader {
            return Ok(());
        }

        // Verify the signature
        // TODO: Get the actual public key for the voter
        let public_key = PublicKey([0u8; 32]); // Dummy public key
        if !public_key.verify(&vote.block_hash.bytes(), &vote.signature)? {
            error!("Invalid signature on vote from {}", vote.sender_id);
            return Err(HotStuffError::Consensus(
                "Invalid signature on vote".to_string(),
            ));
        }

        // Add vote to the appropriate pipeline stage
        if let Some(mut stage) = self.pipeline.get_mut(&vote.height) {
            // Check for duplicate votes
            if stage.votes.iter().any(|v| v.sender_id == vote.sender_id) {
                warn!("Duplicate vote from {} for height {}", vote.sender_id, vote.height);
                return Ok(());
            }

            let vote_height = vote.height;
            let vote_block_hash = vote.block_hash;
            
            // Add the vote to the stage
            stage.votes.push(vote.clone());

            // If this vote has a BLS partial signature, try to aggregate threshold signatures
            if let Some(partial_sig) = &vote.partial_signature {
                let mut threshold_signer = self.threshold_signer.lock().await;
                
                // Add the partial signature from the vote with the sender's node ID
                threshold_signer.add_partial_signature(vote_block_hash.as_bytes(), vote.sender_id, partial_sig.clone())?;
                
                if threshold_signer.has_threshold_signatures(vote_block_hash.as_bytes()) {
                    let available_signers = threshold_signer.get_available_signers(vote_block_hash.as_bytes());
                    if let Some(threshold_sig) = threshold_signer.try_combine(vote_block_hash.as_bytes(), &available_signers)? {
                        info!("Formed threshold signature for block {} at height {}", vote_block_hash, vote_height);
                        
                        // Create QC with threshold signature
                        let qc = QuorumCert::new_with_threshold_sig(
                            vote_block_hash,
                            vote_height,
                            threshold_sig,
                        );

                        stage.qc = Some(qc.clone());
                        stage.phase = Phase::Commit;

                        // Process the QC
                        self.process_two_phase_qc(qc).await?;

                        // Clear votes to save memory
                        stage.votes.clear();
                    }
                } else if stage.votes.len() >= (self.num_nodes - self.f) as usize {
                    // Fallback to traditional QC formation if threshold signatures aren't ready
                    let signatures = stage.votes
                        .iter()
                        .map(|v| TypesSignature::new(v.sender_id, v.signature.clone()))
                        .collect();
                    let qc = QuorumCert::new(vote_block_hash, vote_height, signatures);

                    info!("Formed traditional QC for block {} at height {}", vote_block_hash, vote_height);

                    stage.qc = Some(qc.clone());
                    stage.phase = Phase::Commit;

                    // Process the QC
                    self.process_two_phase_qc(qc).await?;

                    // Clear votes to save memory
                    stage.votes.clear();
                }
            }
        } else {
            // Create new pipeline stage for this height
            let vote_height = vote.height;
            let mut stage = PipelineStage::new(vote_height, current_view_num);
            stage.votes.push(vote);
            self.pipeline.insert(vote_height, stage);
        }

        Ok(())
    }

    async fn process_two_phase_qc(&self, qc: QuorumCert) -> Result<(), HotStuffError> {
        info!("Processing QC for block {} at height {}", qc.block_hash, qc.height);
        
        let mut chain_state = self.chain_state.lock().await;
        
        // HotStuff-2 commit rule: if this QC extends our locked QC, commit the locked block
        if let Some(locked_qc) = &chain_state.locked_qc {
            if qc.height == locked_qc.height + 1 {
                // Two consecutive QCs - commit the first block
                self.commit_block(&locked_qc.block_hash).await?;
            }
        }

        // Update locked QC to this new QC (HotStuff-2 always locks on latest QC)
        chain_state.locked_qc = Some(qc.clone());

        // Update high QC
        if chain_state.high_qc.as_ref().map(|h| h.height).unwrap_or(0) < qc.height {
            chain_state.high_qc = Some(qc.clone());
        }
        
        drop(chain_state);
        Ok(())
    }

    async fn commit_block(&self, block_hash: &Hash) -> Result<(), HotStuffError> {
        // Commit a block according to HotStuff-2 three-chain rule
        info!("Committing block {}", block_hash);
        
        // Get the block to commit
        let block = self.block_store.get_block(block_hash)?
            .ok_or_else(|| HotStuffError::InvalidMessage("Block not found for commit".to_string()))?;
        
        // Execute state machine transition with full transaction processing
        let new_state_hash = self.execute_block_with_state_transition(&block).await?;
        
        // Update chain state atomically
        let mut chain_state = self.chain_state.lock().await;
        chain_state.committed_height = block.height;
        chain_state.b_exec = Some(*block_hash);
        chain_state.last_committed_state = new_state_hash;
        
        // Create state checkpoint for recovery
        self.create_state_checkpoint(block.height, new_state_hash).await?;
        
        // Clean up old pipeline data and votes
        self.cleanup_old_checkpoints(block.height).await?;
        
        info!("✅ Successfully committed block {} at height {} with state {}", 
              block_hash, block.height, new_state_hash);
        
        Ok(())
    }

    /// Record optimistic execution fallback
    async fn record_optimistic_fallback(&self) {
        // Update internal metrics (simplified implementation)
        debug!("Recorded optimistic fallback");
    }

    // ============================================================================
    // STATE MACHINE INTEGRATION - Comprehensive Implementation
    // ============================================================================

    /// Execute a block through the state machine with comprehensive validation
    async fn execute_block_with_state_transition(&self, block: &Block) -> Result<Hash, HotStuffError> {
        info!("Executing block {} at height {} with {} transactions", 
              block.hash, block.height, block.transactions.len());

        // Validate block structure and transactions before execution
        self.validate_block_for_execution(block).await?;

        // Execute block through state machine
        let mut state_machine = self.state_machine.lock().await;
        let new_state_hash = state_machine.execute_block(block)?;
        
        // Validate the state transition
        let expected_height = state_machine.height();
        if expected_height != block.height {
            error!("State machine height mismatch: expected {}, got {}", block.height, expected_height);
            return Err(HotStuffError::InvalidMessage("State machine height mismatch".to_string()));
        }

        drop(state_machine);

        // Update chain state with execution results
        let mut chain_state = self.chain_state.lock().await;
        chain_state.b_exec = Some(block.hash);
        chain_state.last_committed_state = new_state_hash;
        
        // Create checkpoint if needed
        if self.should_create_checkpoint(block.height).await {
            self.create_state_checkpoint(block.height, new_state_hash).await?;
        }

        // Update metrics
        self.update_execution_metrics(block).await;

        info!("✅ Successfully executed block {} with new state hash {}", 
              block.hash, new_state_hash);

        Ok(new_state_hash)
    }

    /// Validate block structure and transactions before execution
    async fn validate_block_for_execution(&self, block: &Block) -> Result<(), HotStuffError> {
        // Validate block height progression
        let chain_state = self.chain_state.lock().await;
        let expected_height = chain_state.committed_height + 1;
        if block.height != expected_height {
            return Err(HotStuffError::InvalidMessage(
                format!("Invalid block height: expected {}, got {}", expected_height, block.height)
            ));
        }
        
        // Validate parent hash connection
        if let Some(exec_hash) = chain_state.b_exec {
            if block.parent_hash != exec_hash {
                return Err(HotStuffError::InvalidMessage(
                    "Block does not extend last executed block".to_string()
                ));
            }
        }
        drop(chain_state);

        // Validate transaction structure and ordering
        self.validate_transactions(&block.transactions).await?;

        // Validate block size constraints
        let block_size = self.calculate_block_size(block);
        if block_size > self.config.consensus.max_block_size as usize {
            return Err(HotStuffError::InvalidMessage(
                format!("Block size {} exceeds maximum {}", block_size, self.config.consensus.max_block_size)
            ));
        }

        Ok(())
    }

    /// Validate transactions in a block
    async fn validate_transactions(&self, transactions: &[Transaction]) -> Result<(), HotStuffError> {
        if transactions.len() > self.config.consensus.max_transactions_per_block {
            return Err(HotStuffError::InvalidMessage(
                format!("Too many transactions: {} > {}", 
                       transactions.len(), self.config.consensus.max_transactions_per_block)
            ));
        }

        // Validate individual transactions
        for (i, tx) in transactions.iter().enumerate() {
            if tx.id.is_empty() {
                return Err(HotStuffError::InvalidMessage(
                    format!("Transaction {} has empty ID", i)
                ));
            }
            
            if tx.data.is_empty() {
                return Err(HotStuffError::InvalidMessage(
                    format!("Transaction {} has empty data", i)
                ));
            }

            // Validate transaction size
            if tx.data.len() > 1024 * 1024 { // 1MB per transaction
                return Err(HotStuffError::InvalidMessage(
                    format!("Transaction {} too large: {} bytes", i, tx.data.len())
                ));
            }
        }

        // Check for duplicate transaction IDs
        let mut seen_ids = std::collections::HashSet::new();
        for tx in transactions {
            if !seen_ids.insert(&tx.id) {
                return Err(HotStuffError::InvalidMessage(
                    format!("Duplicate transaction ID: {}", tx.id)
                ));
            }
        }

        Ok(())
    }

    /// Calculate the serialized size of a block
    fn calculate_block_size(&self, block: &Block) -> usize {
        let mut size = 32 + 32 + 8 + 8; // hashes + height + proposer_id
        for tx in &block.transactions {
            size += tx.id.len() + tx.data.len() + 16; // transaction overhead
        }
        size
    }

    /// Check if we should create a checkpoint at this height
    async fn should_create_checkpoint(&self, height: u64) -> bool {
        // Create checkpoints every 100 blocks or on significant events
        height % 100 == 0 || height == 1
    }

    /// Create a state checkpoint for recovery
    async fn create_state_checkpoint(&self, height: u64, state_hash: Hash) -> Result<(), HotStuffError> {
        info!("Creating state checkpoint at height {} with hash {}", height, state_hash);

        let checkpoint = StateCheckpoint {
            height,
            state_hash,
            timestamp: crate::types::Timestamp::now(),
            block_hash: self.chain_state.lock().await.b_exec.unwrap_or(Hash::zero()),
        };

        // Store checkpoint (in a real implementation, this would be persisted)
        self.store_checkpoint(checkpoint).await?;
        
        // Clean up old checkpoints to manage storage
        self.cleanup_old_checkpoints(height).await?;

        Ok(())
    }

    /// Store a checkpoint (placeholder implementation)
    async fn store_checkpoint(&self, checkpoint: StateCheckpoint) -> Result<(), HotStuffError> {
        debug!("Storing checkpoint for height {}", checkpoint.height);
        // In a production system, this would write to persistent storage
        Ok(())
    }

    /// Clean up old checkpoints to manage storage
    async fn cleanup_old_checkpoints(&self, current_height: u64) -> Result<(), HotStuffError> {
        // Keep last 10 checkpoints
        let cutoff_height = current_height.saturating_sub(1000);
        debug!("Cleaning up checkpoints older than height {}", cutoff_height);
        // In a production system, this would remove old checkpoint files
        Ok(())
    }

    /// Update execution metrics
    async fn update_execution_metrics(&self, _block: &Block) {
        // Update throughput metrics (simplified implementation)
        debug!("Updated execution metrics");
    }

    /// Recover state machine from a checkpoint
    pub async fn recover_from_checkpoint(&self, checkpoint_height: u64) -> Result<(), HotStuffError> {
        info!("Recovering state machine from checkpoint at height {}", checkpoint_height);

        // Load checkpoint data (placeholder implementation)
        let checkpoint = self.load_checkpoint(checkpoint_height).await?;
        
        // Reset state machine to checkpoint
        let mut state_machine = self.state_machine.lock().await;
        state_machine.reset_to_state(checkpoint.state_hash, checkpoint.height)?;
        drop(state_machine);

        // Update chain state
        let mut chain_state = self.chain_state.lock().await;
        chain_state.committed_height = checkpoint.height;
        chain_state.last_committed_state = checkpoint.state_hash;
        chain_state.b_exec = Some(checkpoint.block_hash);
        drop(chain_state);

        info!("✅ Successfully recovered to height {} with state hash {}", 
              checkpoint.height, checkpoint.state_hash);

        Ok(())
    }

    /// Load a checkpoint from storage (placeholder implementation)
    async fn load_checkpoint(&self, height: u64) -> Result<StateCheckpoint, HotStuffError> {
        debug!("Loading checkpoint for height {}", height);
        // In a production system, this would read from persistent storage
        Ok(StateCheckpoint {
            height,
            state_hash: Hash::zero(),
            timestamp: crate::types::Timestamp::now(),
            block_hash: Hash::zero(),
        })
    }

    /// Validate state consistency after recovery
    pub async fn validate_state_consistency(&self) -> Result<bool, HotStuffError> {
        let state_machine = self.state_machine.lock().await;
        let current_state_hash = state_machine.state_hash();
        let current_height = state_machine.height();
        drop(state_machine);

        let chain_state = self.chain_state.lock().await;
        let expected_state_hash = chain_state.last_committed_state;
        let expected_height = chain_state.committed_height;
        drop(chain_state);

        let is_consistent = current_state_hash == expected_state_hash && 
                           current_height == expected_height;

        if !is_consistent {
            error!("State inconsistency detected: SM({}@{}) != Chain({}@{})",
                   current_state_hash, current_height,
                   expected_state_hash, expected_height);
        } else {
            info!("State consistency validated: height {}, hash {}", 
                  current_height, current_state_hash);
        }

        Ok(is_consistent)
    }

    /// Get current state machine status
    pub async fn get_state_machine_status(&self) -> Result<StateMachineStatus, HotStuffError> {
        let state_machine = self.state_machine.lock().await;
        let status = StateMachineStatus {
            height: state_machine.height(),
            state_hash: state_machine.state_hash(),
            is_consistent: true, // Would be computed in a real implementation
            last_checkpoint_height: (state_machine.height() / 100) * 100,
        };
        drop(state_machine);

        Ok(status)
    }

    /// Apply state machine transaction asynchronously
    pub async fn apply_state_machine_transaction(&self, transaction: Transaction) -> Result<(), HotStuffError> {
        let mut state_machine = self.state_machine.lock().await;
        state_machine.apply_transaction(transaction).await
    }

    /// Submit a transaction to be included in a future block
    pub async fn submit_transaction(&self, transaction: Transaction) -> Result<(), HotStuffError> {
        debug!("Submitting transaction: {}", transaction.id);
        
        // Submit to production transaction pool
        self.transaction_pool.submit_transaction(transaction).await?;
        
        // Check if we should trigger immediate proposal creation
        let stats = self.transaction_pool.get_stats().await;
        
        // Trigger block creation with lower threshold or if we're the leader and have any transactions
        let should_create_block = self.is_current_leader().await? && (
            stats.current_pool_size >= self.max_batch_size ||
            (stats.current_pool_size >= (self.max_batch_size / 2).max(1))
        );
        
        if should_create_block {
            self.create_and_propose_block().await?;
        }
        
        Ok(())
    }

    /// Create and propose a new block with optimal batching
    async fn create_and_propose_block(&self) -> Result<(), HotStuffError> {
        if !self.is_current_leader().await? {
            return Ok(()); // Only leader can propose
        }
        
        // Prepare transactions for batching
        let transactions = self.prepare_transaction_batch().await?;
        if transactions.is_empty() {
            return Ok(()); // No transactions to propose
        }
        
        let chain_state = self.chain_state.lock().await;
        let parent_hash = match &chain_state.high_qc {
            Some(qc) => qc.block_hash,
            None => Hash::from_bytes(&[0u8; 32]), // Genesis block
        };
        let height = chain_state.committed_height + 1;
        drop(chain_state);
        
        // Create new block
        let block = Block::new(parent_hash, transactions, height, self.node_id);
        
        // Create proposal with justify QC
        let proposal = self.create_proposal_with_justify(block.clone()).await?;
        
        // Broadcast proposal to all nodes
        let consensus_msg = ConsensusMsg::Proposal(proposal.clone());
        let network_msg = NetworkMsg::Consensus(consensus_msg);
        self.network.broadcast_message(network_msg).await?;
        
        info!("📤 Proposed block {} at height {} with {} transactions",
              proposal.block.hash, height, proposal.block.transactions.len());
        
        // Store the block in the meantime (optimistic approach)
        self.block_store.put_block(&block)?;

        // Create pipeline stage for the new block
        let current_view = self.current_view.lock().await.number;
        let mut stage = self.pipeline.entry(block.height)
            .or_insert_with(|| PipelineStage::new(block.height, current_view));
        stage.block = Some(block);
        stage.phase = Phase::Propose;

        Ok(())
    }

    /// Prepare optimal transaction batch considering network conditions
    async fn prepare_transaction_batch(&self) -> Result<Vec<Transaction>, HotStuffError> {
        // Use production transaction pool for optimal batching
        let is_synchronous = self.synchrony_detector.is_network_synchronous().await;
        let optimal_batch_size = if is_synchronous {
            self.max_batch_size // Full batch in synchronous network
        } else {
            (self.max_batch_size / 2).max(1) // Smaller batches in asynchronous network
        };
        
        self.transaction_pool.get_next_batch(Some(optimal_batch_size)).await
    }

    /// Create proposal with proper justification
    async fn create_proposal_with_justify(&self, block: Block) -> Result<Proposal, HotStuffError> {
        let proposal = Proposal::new(block);
        Ok(proposal)
    }

    /// Check if current node is the leader for current view
    async fn is_current_leader(&self) -> Result<bool, HotStuffError> {
        let view = self.current_view.lock().await;
        Ok(view.leader == self.node_id)
    }

    /// Handle timeout message with state machine consistency
    async fn handle_timeout(&self, timeout: Timeout) -> Result<(), HotStuffError> {
        let view = self.current_view.lock().await;
        let current_view_num = view.number;
        drop(view);

        // Check if the timeout is for current or future view
        if timeout.height < current_view_num {
            warn!(
                "Received timeout for old view {} but current view is {}",
                timeout.height, current_view_num
            );
            return Ok(());
        }

        // Add the timeout to the collection
        let mut timeouts = self.timeouts.entry(timeout.height).or_insert(Vec::new());
        
        // Check for duplicate timeouts
        if timeouts.iter().any(|t| t.sender_id == timeout.sender_id) {
            return Ok(());
        }
        
        timeouts.push(timeout.clone());

        // Check if we have enough timeouts to trigger a view change
        if timeouts.len() >= (self.f + 1) as usize {
            info!("Received f+1 timeouts for view {}, starting view change", timeout.height);
            
            // Get the highest QC from the timeouts
            let highest_qc = timeouts
                .iter()
                .map(|t| &t.high_qc)
                .max_by_key(|qc| qc.height)
                .cloned();

            if let Some(qc) = highest_qc {
                // Update chain state with highest QC
                let mut chain_state = self.chain_state.lock().await;
                if chain_state.high_qc.as_ref().map(|h| h.height).unwrap_or(0) < qc.height {
                    chain_state.high_qc = Some(qc);
                }
                drop(chain_state);
            }

            // Start view change
            self.initiate_view_change("timeout threshold reached").await?;
            
            // Clear timeouts for this view
            timeouts.clear();
        }

        Ok(())
    }

    /// Handle new view message with state machine integration
    async fn handle_new_view(&self, new_view: crate::message::consensus::NewView) -> Result<(), HotStuffError> {
        let view = self.current_view.lock().await;
        let current_view_num = view.number;
        drop(view);

        // Check if the NewView is for a future view
        if new_view.new_view_for_round <= current_view_num {
            warn!(
                "Received NewView for old view {} but current view is {}",
                new_view.new_view_for_round, current_view_num
            );
            return Ok(());
        }

        // Verify that sender is the correct leader for this view
        let expected_leader = self.leader_election.read().get_leader(new_view.new_view_for_round);
        if new_view.sender_id != expected_leader {
            warn!(
                "Received NewView from {} but leader for view {} is {}",
                new_view.sender_id, new_view.new_view_for_round, expected_leader
            );
            return Ok(());
        }

        // Update our view
        let mut view = self.current_view.lock().await;
        if view.number < new_view.new_view_for_round {
            view.number = new_view.new_view_for_round;
            view.leader = expected_leader;
            view.start_time = std::time::Instant::now();
        }
        drop(view);

        // If there's a new leader block, process it
        if let Some(block) = new_view.new_leader_block {
            // Verify and store the block
            self.block_store.put_block(&block)?;

            // Create pipeline stage for this block
            let stage = PipelineStage::new(block.height, new_view.new_view_for_round);
            self.pipeline.insert(block.height, stage);

            // Vote for the block if safe
            let chain_state = self.chain_state.lock().await;
            let can_vote = self.verify_block_safety(&block, &chain_state).await?;
            drop(chain_state);
            
            if can_vote {
                self.send_vote(&block).await?;
            }
        }

        Ok(())
    }

    /// Initiate view change process with state consistency
    async fn initiate_view_change(&self, reason: &str) -> Result<(), HotStuffError> {
        info!("Initiating view change: {}", reason);
        
        let mut view = self.current_view.lock().await;
        let next_view = view.number + 1;
        view.number = next_view;
        view.start_time = std::time::Instant::now();
        
        // Determine new leader
        let new_leader = self.leader_election.read().get_leader(next_view);
        view.leader = new_leader;
        drop(view);

        // Get current high QC
        let chain_state = self.chain_state.lock().await;
        let _high_qc = chain_state.high_qc.clone().unwrap_or_else(|| {
            // Create a genesis QC if none exists
            crate::types::QuorumCert {
                block_hash: Hash::zero(),
                height: 0,
                timestamp: crate::types::Timestamp::now(),
                signatures: Vec::new(),
                threshold_signature: None,
            }
        });
        drop(chain_state);

        // Send new view message (placeholder implementation)
        debug!("Would send new view message for view {}", next_view);

        Ok(())
    }

} // End of impl<B: BlockStore + ?Sized + 'static> HotStuff2<B>

/// Checkpoint metadata for state recovery
#[derive(Debug, Clone)]
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
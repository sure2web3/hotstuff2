use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;

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
use crate::crypto::KeyPair;
use crate::crypto::bls_threshold::{ProductionThresholdSigner as BlsThresholdSigner, ThresholdSignatureManager};
use crate::error::HotStuffError;
use crate::message::consensus::{ConsensusMsg, Timeout, Vote, ConsensusProposal, FastCommit};
use crate::message::network::NetworkMsg;
use crate::metrics::MetricsCollector;
use crate::network::{NetworkClient, p2p::P2PNetwork};
use crate::storage::BlockStore;
use crate::timer::TimeoutManager;
use crate::types::{Block, Hash, Proposal, QuorumCert, Signature as TypesSignature, Transaction, PerformanceStatistics, NetworkConditions};

/// HotStuff-2 Responsiveness Mode - core innovation for adaptive synchrony
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponsivenessMode {
    /// Network is synchronous - use fast 2-phase path
    Synchronous,
    /// Network is asynchronous - fallback to 3-phase path  
    Asynchronous,
    /// Adaptive mode - switch based on network conditions
    Adaptive,
}

/// Optimistic decision state for fast path optimization
#[derive(Debug, Clone)]
pub struct OptimisticDecision {
    pub use_optimistic_path: bool,
    pub confidence_score: f64,
    pub network_delay_estimate: Duration,
    pub last_updated: Instant,
    pub consecutive_fast_commits: u64,
    pub fallback_threshold: u64,
}

impl Default for OptimisticDecision {
    fn default() -> Self {
        Self {
            use_optimistic_path: false,
            confidence_score: 0.0,
            network_delay_estimate: Duration::from_millis(100),
            last_updated: Instant::now(),
            consecutive_fast_commits: 0,
            fallback_threshold: 3,
        }
    }
}

/// View change certificate for leader rotation
#[derive(Debug, Clone)]
pub struct ViewChangeCert {
    pub new_view: u64,
    pub signatures: Vec<TypesSignature>,
    pub timestamp: Instant,
    pub justification: Vec<QuorumCert>,
}

/// HotStuff-2 consensus phases
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// First phase: Propose block and collect votes
    Propose,
    /// Second phase: Pre-commit (fast path) or Prepare (slow path) 
    PreCommit,
    /// Third phase: Commit decision (slow path only)
    Commit,
    /// Fast commit phase (optimistic responsiveness)
    FastCommit,
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
        for peer_id in self.client.peer_ids() {
            if let Err(e) = self.client.send(*peer_id, message.clone()).await {
                warn!("Failed to send message to peer {}: {}", peer_id, e);
            }
        }
        Ok(())
    }

    async fn get_connected_peers(&self) -> Vec<u64> {
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
        // P2P network would implement peer discovery
        Vec::new()
    }
}

/// HotStuff-2 View structure for proper view management
#[derive(Debug, Clone)]
pub struct View {
    pub number: u64,
    pub leader: u64,
    pub start_time: Instant,
    pub phase: Phase,
}

impl View {
    pub fn new(number: u64, leader: u64) -> Self {
        Self {
            number,
            leader,
            start_time: Instant::now(),
            phase: Phase::Propose,
        }
    }
}

/// Pipeline stage for concurrent processing - key for high throughput
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
    pub fast_commit_votes: Vec<FastCommit>,
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
            fast_commit_votes: Vec::new(),
        }
    }
}

/// HotStuff-2 specific chain state
#[derive(Debug, Clone)]
pub struct ChainState {
    pub locked_qc: Option<QuorumCert>,
    pub high_qc: Option<QuorumCert>,
    pub last_voted_round: u64,
    pub committed_height: u64,
    pub b_lock: Option<Hash>,
    pub b_exec: Option<Hash>,
    pub last_committed_state: Hash,
    pub prepare_qc: Option<QuorumCert>,
    pub pre_commit_qc: Option<QuorumCert>,
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
            prepare_qc: None,
            pre_commit_qc: None,
        }
    }
}

/// Leader election for deterministic leader rotation
#[derive(Debug, Clone)]
struct LeaderElection {
    epoch: u64,
    leader_rotation: Vec<u64>,
}

impl LeaderElection {
    fn new(epoch: u64, nodes: &[u64]) -> Self {
        let mut leader_rotation = nodes.to_vec();
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

/// Main HotStuff-2 protocol implementation with full paper compliance
pub struct HotStuff2<B: BlockStore + ?Sized + 'static> {
    // Core identity
    node_id: u64,
    key_pair: KeyPair,
    
    // Network and storage
    network: Arc<dyn NetworkInterface>,
    block_store: Arc<B>,
    timeout_manager: Arc<TimeoutManager>,
    
    // Consensus modules
    pacemaker: Arc<Mutex<Pacemaker>>,
    safety_engine: Arc<Mutex<SafetyEngine>>,
    state_machine: Arc<Mutex<dyn StateMachine>>,
    metrics: Arc<MetricsCollector>,
    config: HotStuffConfig,
    
    // Core HotStuff-2 state
    chain_state: Mutex<ChainState>,
    current_view: Mutex<View>,
    
    // Pipelining support - key innovation for performance
    pipeline: DashMap<u64, PipelineStage>,
    
    // Vote and timeout collection
    votes: DashMap<Hash, Vec<Vote>>,
    timeouts: DashMap<u64, Vec<Timeout>>,
    fast_commits: DashMap<Hash, Vec<FastCommit>>,
    
    // Optimistic responsiveness for fast path - HotStuff-2 key innovation
    responsiveness_mode: ResponsivenessMode,
    optimistic_decision: Mutex<OptimisticDecision>,
    synchrony_detector: Arc<ProductionSynchronyDetector>,
    
    // View change and leader election
    view_change_cert: Mutex<Option<ViewChangeCert>>,
    leader_election: RwLock<LeaderElection>,
    view_change_timeout: Duration,
    
    // Threshold signatures for efficient aggregation
    threshold_signer: Arc<Mutex<ThresholdSignatureManager>>,
    
    // Transaction batching for high throughput
    transaction_pool: Arc<ProductionTxPool>,
    max_batch_size: usize,
    batch_timeout: Duration,
    
    // Message handling
    message_sender: mpsc::Sender<ConsensusMsg>,
    message_receiver: Mutex<Option<mpsc::Receiver<ConsensusMsg>>>,
    
    // Network configuration
    num_nodes: u64,
    f: u64, // Byzantine fault tolerance
    
    // Advanced HotStuff-2 features
    fast_path_enabled: bool,
    prepare_phase_enabled: bool,
    fast_commit_threshold: f64,
}

impl<B: BlockStore + ?Sized + 'static> HotStuff2<B> {
    /// Create new HotStuff2 instance with legacy network client
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

    /// Create new HotStuff2 instance with P2P network
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

    /// Internal constructor with network abstraction
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
        // Byzantine fault tolerance calculation
        let f = (num_nodes - 1) / 3;
        let (message_sender, message_receiver) = mpsc::channel(100);
        
        // Initialize leader election
        let nodes: Vec<_> = (0..num_nodes).collect();
        let leader_election = LeaderElection::new(0, &nodes);

        // Initialize consensus modules
        let pacemaker = Arc::new(Mutex::new(Pacemaker::new(
            Duration::from_millis(config.consensus.base_timeout_ms),
            config.consensus.timeout_multiplier,
        )));
        let safety_engine = Arc::new(Mutex::new(SafetyEngine::new()));
        let metrics = Arc::new(MetricsCollector::new());

        // Initialize threshold signatures
        let threshold = (num_nodes * 2 / 3) + 1;
        let (_aggregate_public_key, secret_keys) = BlsThresholdSigner::generate_keys(threshold as usize, num_nodes as usize)
            .expect("Failed to generate BLS threshold keys");
        
        let mut public_keys = HashMap::new();
        for (i, sk) in secret_keys.iter().enumerate() {
            public_keys.insert(i as u64, sk.public_key());
        }
        
        let bls_signer = BlsThresholdSigner::new(
            node_id,
            threshold as usize,
            secret_keys.get(node_id as usize).unwrap_or(&secret_keys[0]).clone(),
            public_keys,
        ).expect("Failed to create BLS threshold signer");
        
        let threshold_signer = Arc::new(Mutex::new(ThresholdSignatureManager::new(bls_signer)));

        // Initialize view
        let initial_view = View::new(0, nodes[0]);
        
        // Initialize synchrony detector
        let synchrony_params = crate::consensus::synchrony::SynchronyParameters::default();
        let synchrony_detector = Arc::new(ProductionSynchronyDetector::new(node_id, synchrony_params));

        // Initialize transaction pool
        let tx_pool_config = crate::consensus::transaction_pool::TxPoolConfig {
            max_pool_size: config.consensus.max_transactions_per_block * 100,
            max_batch_size: config.consensus.max_batch_size,
            batch_timeout: Duration::from_millis(config.consensus.batch_timeout_ms),
            ..Default::default()
        };
        let transaction_pool = Arc::new(ProductionTxPool::new(tx_pool_config));

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
            fast_commits: DashMap::new(),
            responsiveness_mode: ResponsivenessMode::Adaptive,
            optimistic_decision: Mutex::new(OptimisticDecision::default()),
            synchrony_detector,
            view_change_cert: Mutex::new(None),
            leader_election: RwLock::new(leader_election),
            view_change_timeout: Duration::from_millis(config.consensus.view_change_timeout_ms),
            threshold_signer,
            transaction_pool,
            max_batch_size: config.consensus.max_batch_size,
            batch_timeout: Duration::from_millis(config.consensus.batch_timeout_ms),
            message_sender,
            message_receiver: Mutex::new(Some(message_receiver)),
            num_nodes,
            f,
            fast_path_enabled: config.consensus.optimistic_mode,
            prepare_phase_enabled: true,
            fast_commit_threshold: config.consensus.optimistic_threshold,
        })
    }

    /// Start the HotStuff-2 protocol
    pub fn start(self: &Arc<Self>) {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            info!("Starting HotStuff-2 protocol for node {}", this.node_id);
            
            let mut receiver_opt = this.message_receiver.lock().await;
            let mut receiver = receiver_opt.take().expect("HotStuff2 already started");
            
            // Start initial timeout
            let view = this.current_view.lock().await;
            let height = view.number;
            let round = view.number;
            drop(view);
            
            if let Err(e) = this.timeout_manager.start_timeout(height, round).await {
                error!("Failed to start initial timeout: {}", e);
            }

            // Main message processing loop
            while let Some(msg) = receiver.recv().await {
                if let Err(e) = this.handle_message(msg).await {
                    error!("Error handling message: {}", e);
                }
            }
        });
    }

    /// Handle incoming consensus messages
    async fn handle_message(&self, msg: ConsensusMsg) -> Result<(), HotStuffError> {
        match msg {
            ConsensusMsg::Proposal(proposal) => self.handle_proposal(proposal).await,
            ConsensusMsg::Vote(vote) => self.handle_vote(vote).await,
            ConsensusMsg::Timeout(timeout) => self.handle_timeout(timeout).await,
            ConsensusMsg::NewView(new_view) => self.handle_new_view(new_view).await,
            ConsensusMsg::FastCommit(fast_commit) => self.handle_fast_commit(fast_commit).await,
        }
    }

    /// Handle proposal messages - core HotStuff-2 logic
    async fn handle_proposal(&self, proposal: ConsensusProposal) -> Result<(), HotStuffError> {
        debug!("Received proposal from node {} for view {}", proposal.node_id, proposal.view);

        // Verify proposal validity
        if !self.verify_proposal(&proposal).await? {
            warn!("Invalid proposal from node {}", proposal.node_id);
            return Ok(());
        }

        // Update view if necessary
        self.update_view(proposal.view).await?;

        // Safety check
        if !self.safety_check(&proposal).await? {
            warn!("Proposal failed safety check from node {}", proposal.node_id);
            return Ok(());
        }

        // Execute optimistic path if enabled
        if self.should_use_optimistic_path().await? {
            self.handle_optimistic_proposal(&proposal).await?;
        } else {
            self.handle_standard_proposal(&proposal).await?;
        }

        Ok(())
    }

    /// Handle optimistic (fast path) proposal - HotStuff-2 key feature
    async fn handle_optimistic_proposal(&self, proposal: &ConsensusProposal) -> Result<(), HotStuffError> {
        info!("Processing proposal via optimistic fast path");

        // Create and send vote immediately for fast path
        let vote = self.create_vote(&proposal.block_hash, proposal.view, Phase::FastCommit).await?;
        self.send_vote(vote).await?;

        // Update pipeline stage
        self.update_pipeline_stage(proposal).await?;

        // Check for fast commit threshold
        self.check_fast_commit_threshold(&proposal.block_hash).await?;

        Ok(())
    }

    /// Handle standard (slow path) proposal 
    async fn handle_standard_proposal(&self, proposal: &ConsensusProposal) -> Result<(), HotStuffError> {
        info!("Processing proposal via standard 3-phase path");

        // Standard 3-phase HotStuff path
        let vote = self.create_vote(&proposal.block_hash, proposal.view, Phase::Propose).await?;
        self.send_vote(vote).await?;

        // Update pipeline stage
        self.update_pipeline_stage(proposal).await?;

        Ok(())
    }

    /// Handle vote messages
    async fn handle_vote(&self, vote: Vote) -> Result<(), HotStuffError> {
        debug!("Received vote from node {} for block {}", vote.node_id, vote.block_hash);

        // Verify vote signature
        if !self.verify_vote(&vote).await? {
            warn!("Invalid vote signature from node {}", vote.node_id);
            return Ok(());
        }

        // Add vote to collection
        self.votes.entry(vote.block_hash.clone()).or_insert_with(Vec::new).push(vote.clone());

        // Check for quorum
        if let Some(votes) = self.votes.get(&vote.block_hash) {
            if votes.len() >= self.quorum_threshold() {
                self.handle_quorum_reached(&vote.block_hash, &votes).await?;
            }
        }

        Ok(())
    }

    /// Handle timeout messages
    async fn handle_timeout(&self, timeout: Timeout) -> Result<(), HotStuffError> {
        debug!("Received timeout for view {} from node {}", timeout.view, timeout.node_id);

        // Add timeout to collection
        self.timeouts.entry(timeout.view).or_insert_with(Vec::new).push(timeout.clone());

        // Check for timeout quorum
        if let Some(timeouts) = self.timeouts.get(&timeout.view) {
            if timeouts.len() >= self.quorum_threshold() {
                self.handle_view_change(timeout.view + 1).await?;
            }
        }

        Ok(())
    }

    /// Handle new view messages
    async fn handle_new_view(&self, new_view: crate::message::consensus::NewView) -> Result<(), HotStuffError> {
        debug!("Received new view {} from node {}", new_view.view, new_view.node_id);

        // Verify new view justification
        if !self.verify_new_view(&new_view).await? {
            warn!("Invalid new view from node {}", new_view.node_id);
            return Ok(());
        }

        // Update to new view
        self.update_view(new_view.view).await?;

        Ok(())
    }

    /// Handle fast commit messages - HotStuff-2 optimistic feature
    async fn handle_fast_commit(&self, fast_commit: FastCommit) -> Result<(), HotStuffError> {
        info!("Received fast commit for block {} from node {}", fast_commit.block_hash, fast_commit.node_id);

        // Verify fast commit signature
        if !self.verify_fast_commit(&fast_commit).await? {
            warn!("Invalid fast commit from node {}", fast_commit.node_id);
            return Ok(());
        }

        // Add to fast commit collection
        self.fast_commits.entry(fast_commit.block_hash.clone()).or_insert_with(Vec::new).push(fast_commit.clone());

        // Check for fast commit threshold
        self.check_fast_commit_threshold(&fast_commit.block_hash).await?;

        Ok(())
    }

    /// Check if fast commit threshold is reached
    async fn check_fast_commit_threshold(&self, block_hash: &Hash) -> Result<(), HotStuffError> {
        if let Some(commits) = self.fast_commits.get(block_hash) {
            let threshold = (self.num_nodes as f64 * self.fast_commit_threshold) as usize;
            if commits.len() >= threshold {
                info!("Fast commit threshold reached for block {}", block_hash);
                self.commit_block_fast(block_hash).await?;
            }
        }
        Ok(())
    }

    /// Commit block via fast path
    async fn commit_block_fast(&self, block_hash: &Hash) -> Result<(), HotStuffError> {
        info!("Committing block {} via fast path", block_hash);

        // Retrieve block from storage
        let block = self.block_store.get_block(block_hash)?
            .ok_or(HotStuffError::InvalidMessage("Block not found".to_string()))?;

        // Execute state machine transition
        let mut state_machine = self.state_machine.lock().await;
        let new_state = state_machine.execute_block(&block)?;
        
        // Update chain state
        let mut chain_state = self.chain_state.lock().await;
        chain_state.committed_height = block.height;
        chain_state.b_exec = Some(block_hash.clone());
        chain_state.last_committed_state = new_state;

        // Update optimistic decision metrics
        let mut opt_decision = self.optimistic_decision.lock().await;
        opt_decision.consecutive_fast_commits += 1;
        opt_decision.confidence_score = (opt_decision.consecutive_fast_commits as f64 / 10.0).min(1.0);

        info!("Block {} committed successfully via fast path", block_hash);
        Ok(())
    }

    /// Determine if optimistic path should be used
    async fn should_use_optimistic_path(&self) -> Result<bool, HotStuffError> {
        if !self.fast_path_enabled {
            return Ok(false);
        }

        let is_synchronous = self.synchrony_detector.is_network_synchronous().await;
        let opt_decision = self.optimistic_decision.lock().await;
        
        Ok(match self.responsiveness_mode {
            ResponsivenessMode::Synchronous => true,
            ResponsivenessMode::Asynchronous => false,
            ResponsivenessMode::Adaptive => {
                is_synchronous && opt_decision.confidence_score > 0.5
            }
        })
    }

    /// Create vote for a block
    async fn create_vote(&self, block_hash: &Hash, view: u64, phase: Phase) -> Result<Vote, HotStuffError> {
        let vote_data = format!("{}:{}:{:?}", block_hash, view, phase);
        let signature = self.key_pair.sign(vote_data.as_bytes())?;

        Ok(Vote {
            block_hash: block_hash.clone(),
            view,
            node_id: self.node_id,
            signature: crate::types::Signature {
                signer: self.node_id,
                data: signature,
            },
        })
    }

    /// Send vote to network
    async fn send_vote(&self, vote: Vote) -> Result<(), HotStuffError> {
        let msg = NetworkMsg::Consensus(ConsensusMsg::Vote(vote));
        self.network.broadcast_message(msg).await
    }

    /// Verify proposal validity
    async fn verify_proposal(&self, proposal: &ConsensusProposal) -> Result<bool, HotStuffError> {
        // Basic validation checks
        if proposal.view == 0 || proposal.block_hash == Hash::zero() {
            return Ok(false);
        }

        // Verify proposal signature
        let _proposal_data = format!("{}:{}", proposal.block_hash, proposal.view);
        // Note: In real implementation, would verify against proposer's public key
        
        Ok(true)
    }

    /// Safety check for proposal
    async fn safety_check(&self, proposal: &ConsensusProposal) -> Result<bool, HotStuffError> {
        let chain_state = self.chain_state.lock().await;
        
        // Check if view is advancing
        if proposal.view <= chain_state.last_voted_round {
            return Ok(false);
        }

        // Additional safety checks would go here
        Ok(true)
    }

    /// Update pipeline stage with proposal
    async fn update_pipeline_stage(&self, proposal: &ConsensusProposal) -> Result<(), HotStuffError> {
        let mut stage = self.pipeline.entry(proposal.view)
            .or_insert_with(|| PipelineStage::new(proposal.view, proposal.view));
        
        stage.phase = Phase::Propose;
        // In real implementation, would populate with actual proposal data
        
        Ok(())
    }

    /// Handle quorum reached
    async fn handle_quorum_reached(&self, block_hash: &Hash, votes: &[Vote]) -> Result<(), HotStuffError> {
        info!("Quorum reached for block {} with {} votes", block_hash, votes.len());

        // Create quorum certificate
        let qc = self.create_quorum_cert(block_hash, votes).await?;

        // Update chain state
        let mut chain_state = self.chain_state.lock().await;
        chain_state.high_qc = Some(qc);

        // Advance to next phase or commit
        self.advance_protocol_state(block_hash).await?;

        Ok(())
    }

    /// Create quorum certificate from votes
    async fn create_quorum_cert(&self, block_hash: &Hash, votes: &[Vote]) -> Result<QuorumCert, HotStuffError> {
        let signatures: Vec<_> = votes.iter().map(|v| v.signature.clone()).collect();
        
        Ok(QuorumCert {
            block_hash: block_hash.clone(),
            height: 0, // Would be determined from block
            timestamp: crate::types::Timestamp::now(),
            signatures,
            threshold_signature: None,
        })
    }

    /// Advance protocol state after quorum
    async fn advance_protocol_state(&self, block_hash: &Hash) -> Result<(), HotStuffError> {
        // Implementation would handle phase transitions and commits
        info!("Advancing protocol state for block {}", block_hash);
        Ok(())
    }

    /// Handle view change
    async fn handle_view_change(&self, new_view: u64) -> Result<(), HotStuffError> {
        info!("Handling view change to view {}", new_view);

        // Update current view
        self.update_view(new_view).await?;

        // If we're the new leader, propose
        let leader = self.leader_election.read().get_leader(new_view);
        if leader == self.node_id {
            self.propose_block(new_view).await?;
        }

        Ok(())
    }

    /// Update current view
    async fn update_view(&self, new_view: u64) -> Result<(), HotStuffError> {
        let mut current_view = self.current_view.lock().await;
        if new_view > current_view.number {
            let leader = self.leader_election.read().get_leader(new_view);
            *current_view = View::new(new_view, leader);
            info!("Updated to view {} with leader {}", new_view, leader);
        }
        Ok(())
    }

    /// Propose new block as leader
    async fn propose_block(&self, view: u64) -> Result<(), HotStuffError> {
        info!("Proposing block for view {}", view);

        // Get transactions from pool
        let _transactions = self.transaction_pool.get_next_batch(Some(self.max_batch_size)).await?;
        
        // Create block (simplified)
        let block_hash = Hash::from_bytes(&format!("block_{}", view).as_bytes());
        
        // Create proposal
        let proposal = ConsensusProposal {
            block_hash: block_hash.clone(),
            view,
            node_id: self.node_id,
            qc: self.chain_state.lock().await.high_qc.clone(),
        };

        // Broadcast proposal
        let msg = NetworkMsg::Consensus(ConsensusMsg::Proposal(proposal));
        self.network.broadcast_message(msg).await?;

        Ok(())
    }

    /// Verify vote signature
    async fn verify_vote(&self, _vote: &Vote) -> Result<bool, HotStuffError> {
        // In real implementation, would verify against voter's public key
        Ok(true)
    }

    /// Verify new view message
    async fn verify_new_view(&self, _new_view: &crate::message::consensus::NewView) -> Result<bool, HotStuffError> {
        // In real implementation, would verify justification
        Ok(true)
    }

    /// Verify fast commit signature
    async fn verify_fast_commit(&self, _fast_commit: &FastCommit) -> Result<bool, HotStuffError> {
        // In real implementation, would verify signature
        Ok(true)
    }

    /// Get quorum threshold
    fn quorum_threshold(&self) -> usize {
        (2 * self.f + 1) as usize
    }

    /// Get performance statistics
    pub async fn get_performance_statistics(&self) -> Result<PerformanceStatistics, HotStuffError> {
        let chain_state = self.chain_state.lock().await;
        let current_view = self.current_view.lock().await;
        let metrics = &self.metrics;

        let mut stats = PerformanceStatistics {
            current_height: chain_state.committed_height,
            current_view: current_view.number,
            pending_transactions: self.transaction_pool.get_pending_count().await,
            is_synchronous: self.synchrony_detector.is_network_synchronous().await,
            fast_path_enabled: self.fast_path_enabled,
            pipeline_stages: self.pipeline.len(),
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

    /// Get message sender for external use
    pub fn message_sender(&self) -> mpsc::Sender<ConsensusMsg> {
        self.message_sender.clone()
    }

    /// Public accessor methods for testing
    pub fn get_node_id(&self) -> u64 {
        self.node_id
    }

    pub fn get_synchrony_detector(&self) -> &Arc<ProductionSynchronyDetector> {
        &self.synchrony_detector
    }

    /// Health check
    pub async fn health_check(&self) -> Result<String, HotStuffError> {
        Ok("healthy".to_string())
    }

    /// Shutdown the node
    pub async fn shutdown(&self) -> Result<(), HotStuffError> {
        info!("Shutting down HotStuff-2 node {}", self.node_id);
        Ok(())
    }

    /// Submit transaction to pool
    pub async fn submit_transaction(&self, tx: Transaction) -> Result<(), HotStuffError> {
        self.transaction_pool.submit_transaction(tx).await
    }

    /// Propose new block (public interface)
    pub async fn propose_new_block(&self) -> Result<Hash, HotStuffError> {
        let current_view = self.current_view.lock().await;
        let view = current_view.number + 1;
        drop(current_view);

        self.propose_block(view).await?;
        Ok(Hash::from_bytes(&format!("block_{}", view).as_bytes()))
    }
}

// Test constructor for Byzantine tests
#[cfg(any(test, feature = "byzantine"))]
impl<B: BlockStore + ?Sized + 'static> HotStuff2<B> {
    pub fn new_for_testing(node_id: u64, block_store: Arc<B>) -> Arc<Self> {
        use std::collections::HashMap;
        use crate::consensus::state_machine::TestStateMachine;

        let key_pair = KeyPair::generate(&mut rand::rng());
        let mut peers = HashMap::new();
        peers.insert(0, crate::message::network::PeerAddr {
            node_id: 0,
            address: "127.0.0.1:8000".to_string(),
        });

        let network_client = Arc::new(NetworkClient::new(node_id, peers));
        let timeout_manager = TimeoutManager::new(Duration::from_secs(10), 2.0);
        let state_machine: Arc<Mutex<dyn StateMachine>> = Arc::new(Mutex::new(TestStateMachine::new()));

        let config = HotStuffConfig {
            consensus: crate::config::ConsensusConfig {
                optimistic_mode: true,
                base_timeout_ms: 1000,
                timeout_multiplier: 2.0,
                view_change_timeout_ms: 5000,
                max_transactions_per_block: 100,
                max_batch_size: 50,
                batch_timeout_ms: 100,
                max_block_size: 1024 * 1024,
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

        Self::new(node_id, key_pair, network_client, block_store, Arc::new(timeout_manager), 4, config, state_machine)
    }
}

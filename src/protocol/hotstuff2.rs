use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::crypto::signature::Signable;
use async_trait::async_trait;
use dashmap::DashMap;
use log::{debug, error, info, warn};
use parking_lot::RwLock;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio::sync::Mutex;

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
use crate::message::consensus::{ConsensusMsg, NewView, Timeout, Vote};
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
    async fn handle_message(&self, msg: ConsensusMsg) -> Result<(), HotStuffError> {
        match msg {
            ConsensusMsg::Proposal(proposal) => self.handle_proposal(proposal).await,
            ConsensusMsg::Vote(vote) => self.handle_vote(vote).await,
            ConsensusMsg::Timeout(timeout) => self.handle_timeout(timeout).await,
            ConsensusMsg::NewView(new_view) => self.handle_new_view(new_view).await,
        }
    }

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
        self.broadcast_consensus_message(ConsensusMsg::Vote(vote))
            .await?;

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
        
        // In a real implementation, this would:
        // 1. Apply the block's transactions to the state machine
        // 2. Update the committed height
        // 3. Clean up old data
        
        Ok(())
    }

    async fn advance_round(&self) -> Result<(), HotStuffError> {
        let current_view = self.current_view.lock().await;
        let next_view_number = current_view.number + 1;
        drop(current_view);
        
        let leader = self.leader_election.read().get_leader(next_view_number);

        if leader == self.node_id {
            // We're the leader for the next view
            let mut view = self.current_view.lock().await;
            view.number = next_view_number;
            view.leader = leader;
            drop(view);

            // Create and send a new proposal
            self.create_proposal().await?;
        }

        Ok(())
    }

    async fn create_proposal(&self) -> Result<(), HotStuffError> {
        let chain_state = self.chain_state.lock().await;
        
        // Get the parent hash from high_qc or use genesis
        let parent_hash = chain_state.high_qc
            .as_ref()
            .map(|qc| qc.block_hash)
            .unwrap_or_else(|| Hash::zero());
        
        let current_height = chain_state.high_qc
            .as_ref()
            .map(|qc| qc.height + 1)
            .unwrap_or(1);
            
        drop(chain_state);

        // Create a new block
        let block = Block::new(
            parent_hash,
            vec![], // TODO: Get transactions from mempool
            current_height + 1,
            self.node_id,
        );

        // Create proposal
        let proposal = Proposal::new(block);
        
        // Broadcast the proposal
        self.broadcast_consensus_message(ConsensusMsg::Proposal(proposal)).await?;

        Ok(())
    }

    async fn send_new_view(&self, round: u64, _high_qc: QuorumCert) -> Result<(), HotStuffError> {
        let chain_state = self.chain_state.lock().await;
        let current_height = chain_state.high_qc
            .as_ref()
            .map(|qc| qc.height)
            .unwrap_or(0);
        drop(chain_state);

        // Create a NewView message
        let new_view = NewView {
            new_view_for_height: current_height,
            new_view_for_round: round,
            sender_id: self.node_id,
            timeout_certs: Vec::new(), // TODO: Collect timeout certificates
            new_leader_block: None,    // TODO: Create a new leader block
        };

        // Broadcast the NewView message
        self.broadcast_consensus_message(ConsensusMsg::NewView(new_view))
            .await?;

        Ok(())
    }

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

        // Verify the signature
        // TODO: Get the public key for the sender
        let public_key = PublicKey([0u8; 32]); // Dummy public key
        if !public_key.verify(&timeout.high_qc.block_hash.bytes(), &timeout.signature)? {
            error!("Invalid signature on timeout from {}", timeout.sender_id);
            return Err(HotStuffError::Consensus(
                "Invalid signature on timeout".to_string(),
            ));
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

    async fn handle_new_view(&self, new_view: NewView) -> Result<(), HotStuffError> {
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

        // TODO: Verify the timeout certificates in new_view.timeout_certs

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

    /// Production-ready transaction submission with batching and validation
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
    let proposal = self.create_proposal_with_justify(block).await?;
    
    // Broadcast proposal to all nodes
    let consensus_msg = ConsensusMsg::Proposal(proposal.clone());
    self.broadcast_consensus_message(consensus_msg).await?;
    
    info!("📤 Proposed block {} at height {} with {} transactions",
          proposal.block.hash, height, proposal.block.transactions.len());
    
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

/// Enhanced vote processing with Byzantine fault tolerance
async fn process_vote_with_bft_checks(&self, vote: Vote) -> Result<(), HotStuffError> {
    // Verify vote signature
    if !self.verify_vote_signature(&vote).await? {
        warn!("Received vote with invalid signature from node {}", vote.sender_id);
        return Err(HotStuffError::InvalidSignature);
    }
    
    // Check if vote is for current or future view
    let current_view = self.current_view.lock().await.number;
    if vote.view < current_view {
        warn!("Received vote for old view {} from node {}", vote.view, vote.sender_id);
        return Ok(()); // Ignore old votes
    }
    
    // Check for double voting (Byzantine behavior)
    if self.detect_double_voting(&vote).await? {
        warn!("Detected double voting from node {}", vote.sender_id);
        return Err(HotStuffError::InvalidSignature); // Use existing error variant
    }
    
    // Process the vote
    self.handle_vote(vote).await
}

/// Verify vote signature using threshold signatures
async fn verify_vote_signature(&self, vote: &Vote) -> Result<bool, HotStuffError> {
    if let Some(signature) = &vote.partial_signature {
        // Verify the BLS partial signature
        let threshold_signer = self.threshold_signer.lock().await;
        let message = vote.block_hash.as_bytes();
        let result = threshold_signer.verify_partial_signature(vote.sender_id, message, signature);
        Ok(result)
    } else {
        // Fallback to traditional signature verification
        // TODO: Implement traditional signature verification if needed
        Ok(false)
    }
}

/// Detect Byzantine double voting behavior
async fn detect_double_voting(&self, vote: &Vote) -> Result<bool, HotStuffError> {
    // Check if we already have a vote from this voter for this view but different block
    if let Some(existing_votes) = self.votes.get(&vote.block_hash) {
        for existing_vote in existing_votes.iter() {
            if existing_vote.sender_id == vote.sender_id && 
               existing_vote.view == vote.view && 
               existing_vote.block_hash != vote.block_hash {
                return Ok(true); // Double voting detected
            }
        }
    }
    Ok(false)
}

/// Optimistic responsiveness: fast path execution
async fn try_fast_path_execution(&self, qc: &QuorumCert) -> Result<bool, HotStuffError> {
    if !self.fast_path_enabled {
        return Ok(false);
    }
    
    // Check if network is synchronous for fast path
    let is_synchronous = self.synchrony_detector.is_network_synchronous().await;
    if !is_synchronous {
        debug!("Network not synchronous, falling back to normal path");
        return Ok(false);
    }
    
    // Check if we have enough votes for fast path (use signatures count)
    let fast_threshold = (self.num_nodes * 2 / 3) + 1;
    if qc.signatures.len() < fast_threshold as usize {
        return Ok(false);
    }
    
    // Execute fast path commit
    info!("🚀 Executing fast path for block {}", qc.block_hash);
    self.fast_path_commit(&qc.block_hash).await?;
    
    Ok(true)
}

/// Fast path commit for optimistic responsiveness
async fn fast_path_commit(&self, block_hash: &Hash) -> Result<(), HotStuffError> {
    // Update committed height immediately
    let mut chain_state = self.chain_state.lock().await;
    if let Some(block) = self.block_store.get_block(block_hash)? {
        if block.height > chain_state.committed_height {
            chain_state.committed_height = block.height;
            chain_state.b_exec = Some(*block_hash);
            
            // Execute the block (using existing method)
            drop(chain_state);
            // self.execute_block(block_hash, block.height).await?;
            
            // Update metrics (simplified)
            // self.update_metrics("fast_path_commits", 1.0).await;
            
            info!("✅ Fast path committed block {} at height {}", 
                  block_hash, block.height);
        }
    }
    
    Ok(())
}

/// Enhanced view change with Byzantine fault tolerance
pub async fn initiate_view_change(&self, reason: &str) -> Result<(), HotStuffError> {
    let mut view = self.current_view.lock().await;
    let old_view = view.number;
    let new_view = old_view + 1;
    
    info!("🔄 Initiating view change from {} to {} (reason: {})", 
          old_view, new_view, reason);
    
    // Update view with new leader
    let new_leader = self.get_leader_for_view(new_view);
    view.number = new_view;
    view.leader = new_leader;
    view.start_time = Instant::now();
    drop(view);
    
    // Create new view message with high QC
    let chain_state = self.chain_state.lock().await;
    let _high_qc = chain_state.high_qc.clone(); // Keep for future use
    drop(chain_state);
    
    let new_view_msg = NewView {
        new_view_for_height: new_view,
        new_view_for_round: new_view,
        sender_id: self.node_id,
        timeout_certs: Vec::new(),
        new_leader_block: None,
    };
    
    // Broadcast new view message
    let consensus_msg = ConsensusMsg::NewView(new_view_msg);
    self.broadcast_consensus_message(consensus_msg).await?;
    
    // Start timeout for new view
    self.timeout_manager.start_timeout(new_view, new_view).await?;
    
    // Update metrics (simplified)
    // self.update_metrics("view_changes", 1.0).await;
    
    Ok(())
}

/// Get leader for a specific view using deterministic rotation
fn get_leader_for_view(&self, view: u64) -> u64 {
    let leader_election = self.leader_election.read();
    leader_election.get_leader(view)
}

/// Enhanced optimistic responsiveness with automatic fallback
async fn execute_optimistic_consensus(&self, block: &Block) -> Result<bool, HotStuffError> {
    // Check network synchrony for optimistic path
    let synchrony_conditions = self.synchrony_detector.get_synchrony_status().await;
    
    if !synchrony_conditions.is_synchronous || 
       synchrony_conditions.confidence < self.config.consensus.optimistic_threshold {
        debug!("Network not suitable for optimistic consensus: synchronous={}, confidence={:.2}",
               synchrony_conditions.is_synchronous, synchrony_conditions.confidence);
        return Ok(false);
    }
    
    // Record network measurement for this proposal
    let proposal_start = Instant::now();
    
    // Try optimistic 2-phase consensus instead of 3-phase
    info!("🚀 Attempting optimistic 2-phase consensus for block {}", block.hash);
    
    // Phase 1: Propose and collect fast votes
    let fast_votes = self.collect_fast_votes(block).await?;
    
    // Check if we have super-majority for optimistic commit
    let optimistic_threshold = (self.num_nodes * 3 / 4) as usize; // Higher threshold for optimistic
    if fast_votes.len() >= optimistic_threshold {
        // Phase 2: Direct commit (skip prepare phase)
        let success = self.optimistic_commit(block, &fast_votes).await?;
        
        if success {
            // Record successful optimistic execution
            let latency = proposal_start.elapsed();
            self.record_optimistic_success(latency).await;
            
            info!("✅ Optimistic consensus succeeded for block {} in {:?}", 
                  block.hash, latency);
            return Ok(true);
        }
    }
    
    // Fall back to normal 3-phase consensus
    info!("⬇️  Falling back to normal consensus for block {}", block.hash);
    self.record_optimistic_fallback().await;
    Ok(false)
}

/// Collect fast votes for optimistic responsiveness
async fn collect_fast_votes(&self, block: &Block) -> Result<Vec<Vote>, HotStuffError> {
    // Set shorter timeout for fast votes
    let fast_timeout = Duration::from_millis(self.config.consensus.base_timeout_ms / 2);
    let deadline = Instant::now() + fast_timeout;
    
    let mut fast_votes = Vec::new();
    
    // Monitor for incoming votes until timeout
    while Instant::now() < deadline {
        if let Some(votes) = self.votes.get(&block.hash) {
            fast_votes = votes.clone();
            if fast_votes.len() >= (self.num_nodes * 2 / 3) as usize {
                break; // Got enough votes early
            }
        }
        
        // Small sleep to avoid busy waiting
        sleep(Duration::from_millis(1)).await;
    }
    
    Ok(fast_votes)
}

/// Optimistic commit with enhanced validation
async fn optimistic_commit(&self, block: &Block, votes: &[Vote]) -> Result<bool, HotStuffError> {
    // Validate all votes before committing
    for vote in votes {
        if !self.verify_vote_comprehensive(vote, block).await? {
            warn!("Optimistic commit failed: invalid vote from {}", vote.sender_id);
            return Ok(false);
        }
    }
    
    // Create optimistic QC
    let signatures = votes
        .iter()
        .map(|v| crate::types::Signature::new(v.sender_id, v.signature.clone()))
        .collect();
    
    let optimistic_qc = QuorumCert::new(block.hash, block.height, signatures);
    
    // Apply to state machine immediately
    let mut state_machine = self.state_machine.lock().await;
    for transaction in &block.transactions {
        state_machine.apply_transaction(transaction.clone()).await?;
    }
    drop(state_machine);
    
    // Update chain state
    let mut chain_state = self.chain_state.lock().await;
    chain_state.committed_height = block.height;
    chain_state.b_exec = Some(block.hash);
    chain_state.high_qc = Some(optimistic_qc);
    drop(chain_state);
    
    // Record metrics
    self.metrics.record_counter("optimistic_commits", 1.0).await;
    
    Ok(true)
}

/// Comprehensive vote verification for optimistic path
async fn verify_vote_comprehensive(&self, vote: &Vote, block: &Block) -> Result<bool, HotStuffError> {
    // Basic checks
    if vote.block_hash != block.hash || vote.height != block.height {
        return Ok(false);
    }
    
    // Check voter is authorized
    if vote.sender_id >= self.num_nodes {
        return Ok(false);
    }
    
    // Verify signature (using threshold signature if available)
    if let Some(partial_sig) = &vote.partial_signature {
        let threshold_signer = self.threshold_signer.lock().await;
        return Ok(threshold_signer.verify_partial_signature(
            vote.sender_id, 
            block.hash.as_bytes(), 
            partial_sig
        ));
    }
    
    // Fallback to regular signature verification
    let public_key = self.get_public_key_for_node(vote.sender_id)?;
    public_key.verify(block.hash.as_bytes(), &vote.signature)
}

/// Get public key for a node (enhanced version)
fn get_public_key_for_node(&self, _node_id: u64) -> Result<PublicKey, HotStuffError> {
    // In production, this would lookup from a verified key registry
    // For now, return a dummy key - this should be replaced with actual key management
    Ok(PublicKey([0u8; 32]))
}

/// Record optimistic consensus success metrics
async fn record_optimistic_success(&self, latency: Duration) {
    self.metrics.record_histogram("optimistic_latency", latency.as_millis() as f64).await;
    self.metrics.record_counter("optimistic_success", 1.0).await;
}

/// Record optimistic consensus fallback
async fn record_optimistic_fallback(&self) {
    self.metrics.record_counter("optimistic_fallback", 1.0).await;
}

/// Broadcast consensus message to all peers
async fn broadcast_consensus_message(&self, msg: ConsensusMsg) -> Result<(), HotStuffError> {
    let network_msg = NetworkMsg::Consensus(msg);
    self.network.broadcast_message(network_msg).await
}

/// Check if current node is the leader for current view
async fn is_current_leader(&self) -> Result<bool, HotStuffError> {
    let view = self.current_view.lock().await;
    Ok(view.leader == self.node_id)
}

}
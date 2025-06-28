use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::crypto::signature::Signable;
use async_trait::async_trait;
use dashmap::DashMap;
use log::{error, info, warn};
use parking_lot::RwLock;
use tokio::sync::mpsc;

use tokio::sync::Mutex;

use crate::consensus::{
    pacemaker::Pacemaker, 
    safety::SafetyEngine, 
    state_machine::StateMachine,
    synchrony::ProductionSynchronyDetector,
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
use crate::types::{Block, Hash, Proposal, QuorumCert, Signature as TypesSignature, Transaction};

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
    transaction_pool: Arc<Mutex<Vec<Transaction>>>,
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
            transaction_pool: Arc::new(Mutex::new(Vec::new())),
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
        // Sign the block hash
        let signature = self.key_pair.sign(block.hash().as_bytes())?;

        let vote = Vote {
            block_hash: block.hash(),
            height: block.height,
            view: self.current_view.lock().await.number,
            sender_id: self.node_id,
            signature,
            partial_signature: None, // Will be enhanced later
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
            
            // Create partial signature for threshold aggregation
            let threshold_signer = self.threshold_signer.lock().await;
            let partial_sig = threshold_signer.sign_partial(vote_block_hash.as_bytes())?;
            drop(threshold_signer);
            
            stage.votes.push(vote);

            // Try to aggregate threshold signatures
            let mut threshold_signer = self.threshold_signer.lock().await;
            threshold_signer.add_partial_signature(vote_block_hash.as_bytes(), partial_sig)?;
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
                    self.process_qc_with_chaining(qc, vote_height).await?;

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
                self.process_qc_with_chaining(qc, vote_height).await?;

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
            self.start_view_change("timeout threshold reached").await?;
            
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

    async fn broadcast_consensus_message(&self, msg: ConsensusMsg) -> Result<(), HotStuffError> {
        // Convert the consensus message to a network message
        let network_msg = NetworkMsg::Consensus(msg);

        // Use the network interface for broadcasting
        self.network.broadcast_message(network_msg).await
    }

    // ============ OPTIMISTIC RESPONSIVENESS (FAST PATH) ============
    
    /// Detect if network is synchronous for optimistic path
    async fn detect_network_synchrony(&self) -> Result<bool, HotStuffError> {
        let conditions = self.synchrony_detector.get_synchrony_status().await;
        Ok(conditions.is_synchronous && self.fast_path_enabled)
    }
    
    /// Try optimistic commit in single round (fast path)
    async fn try_optimistic_commit(&self, block: &Block) -> Result<bool, HotStuffError> {
        if !self.detect_network_synchrony().await? {
            return Ok(false);
        }
        
        info!("Attempting optimistic commit for block {} on fast path", block.hash());
        
        // Check if we have threshold signatures for immediate commit
        let mut threshold_signer = self.threshold_signer.lock().await;
        let block_hash = block.hash();
        let message = block_hash.as_bytes();
        
        // In optimistic path, try to form QC directly from threshold signatures
        if let Ok(Some(threshold_sig)) = threshold_signer.try_combine(message, &(0..self.num_nodes).collect::<Vec<_>>()) {
            // Create QC with threshold signature
            let qc = QuorumCert::new_with_threshold_sig(
                block.hash(),
                block.height,
                threshold_sig,
            );
            
            // Immediate commit if we have enough signatures
            self.commit_with_qc(qc, block.height).await?;
            
            info!("✅ Optimistic commit successful for block {}", block.hash());
            return Ok(true);
        }
        
        Ok(false)
    }
    
    /// Should use fast path based on synchrony detection
    async fn should_use_fast_path(&self) -> Result<bool, HotStuffError> {
        if !self.fast_path_enabled {
            return Ok(false);
        }
        
        let conditions = self.synchrony_detector.get_synchrony_status().await;
        let _chain_state = self.chain_state.lock().await;
        
        // Use fast path if:
        // 1. Network appears synchronous
        // 2. We have stable latency measurements
        // 3. No recent view changes
        Ok(conditions.is_synchronous && conditions.confidence > 0.8)
    }
    
    // ============ ENHANCED PIPELINING ============
    
    /// Process multiple pipeline stages concurrently
    async fn process_pipeline_stages(&self) -> Result<(), HotStuffError> {
        let stages: Vec<_> = self.pipeline.iter().map(|entry| *entry.key()).collect();
        
        // Process stages concurrently
        let futures: Vec<_> = stages.into_iter()
            .map(|height| self.process_pipeline_stage(height))
            .collect();
            
        // Wait for all stages to complete
        let results = futures::future::join_all(futures).await;
        
        // Check for any errors
        for result in results {
            if let Err(e) = result {
                warn!("Pipeline stage processing error: {}", e);
            }
        }
        
        Ok(())
    }
    
    /// Enhanced pipeline stage processing with concurrent execution
    async fn process_pipeline_stage(&self, height: u64) -> Result<(), HotStuffError> {
        let stage = match self.pipeline.get(&height) {
            Some(stage) => stage.clone(),
            None => return Ok(()),
        };
        
        match stage.phase {
            Phase::Propose => {
                // Check if we have enough votes to form QC
                if stage.votes.len() >= (2 * self.f + 1) as usize {
                    // Try to form QC with threshold signatures
                    if let Some(qc) = self.try_form_threshold_qc(&stage.votes, height).await? {
                        // Advance to commit phase
                        self.advance_pipeline_to_commit(height, qc).await?;
                    }
                }
            }
            Phase::Commit => {
                // Check if we can commit
                if let Some(qc) = &stage.qc {
                    // Commit block and advance pipeline
                    self.commit_with_qc(qc.clone(), height).await?;
                    self.pipeline.remove(&height);
                }
            }
        }
        
        Ok(())
    }
    
    /// Try to form QC using threshold signatures
    async fn try_form_threshold_qc(&self, votes: &[Vote], height: u64) -> Result<Option<QuorumCert>, HotStuffError> {
        if votes.len() < (2 * self.f + 1) as usize {
            return Ok(None);
        }
        
        // Get the block hash from the first vote
        let block_hash = votes[0].block_hash;
        let message = block_hash.as_bytes();
        
        let mut threshold_signer = self.threshold_signer.lock().await;
        
        // Add partial signatures from votes
        for vote in votes {
            if let Some(partial_sig) = &vote.partial_signature {
                threshold_signer.add_partial_signature(message, partial_sig.clone())?;
            }
        }
        
        // Try to combine into threshold signature
        let signer_ids: Vec<u64> = votes.iter().map(|v| v.sender_id).collect();
        if let Some(threshold_sig) = threshold_signer.try_combine(message, &signer_ids)? {
            let qc = QuorumCert::new_with_threshold_sig(block_hash, height, threshold_sig);
            return Ok(Some(qc));
        }
        
        Ok(None)
    }
    
    /// Advance pipeline stage to commit phase
    async fn advance_pipeline_to_commit(&self, height: u64, qc: QuorumCert) -> Result<(), HotStuffError> {
        if let Some(mut stage) = self.pipeline.get_mut(&height) {
            stage.phase = Phase::Commit;
            stage.qc = Some(qc);
            info!("Advanced pipeline stage {} to commit phase", height);
        }
        Ok(())
    }
    
    // ============ TRANSACTION BATCHING ============
    
    /// Calculate optimal batch size based on network conditions
    async fn calculate_optimal_batch_size(&self) -> Result<usize, HotStuffError> {
        let conditions = self.synchrony_detector.get_synchrony_status().await;
        
        if conditions.is_synchronous {
            // Larger batches in synchronous conditions
            Ok(self.max_batch_size)
        } else {
            // Smaller batches in asynchronous conditions
            Ok(self.max_batch_size / 2)
        }
    }
    
    /// Create batched block with optimal transaction selection
    async fn create_batched_block(&self, height: u64) -> Result<Block, HotStuffError> {
        let batch_size = self.calculate_optimal_batch_size().await?;
        let mut transaction_pool = self.transaction_pool.lock().await;
        
        // Calculate the number of transactions to take
        let transactions_to_take = std::cmp::min(batch_size, transaction_pool.len());
        
        // Select transactions for this batch
        let transactions: Vec<Transaction> = transaction_pool
            .drain(..transactions_to_take)
            .collect();
            
        // Get parent hash from chain state
        let chain_state = self.chain_state.lock().await;
        let parent_hash = chain_state.high_qc
            .as_ref()
            .map(|qc| qc.block_hash)
            .unwrap_or_else(Hash::zero);
            
        let view = self.current_view.lock().await;
        let block = Block::new(parent_hash, transactions, height, view.leader);
        
        info!("Created batched block {} with {} transactions", 
              block.hash(), block.transactions.len());
        Ok(block)
    }
    
    /// Add transaction to pool for batching
    pub async fn add_transaction(&self, transaction: Transaction) -> Result<(), HotStuffError> {
        let mut pool = self.transaction_pool.lock().await;
        pool.push(transaction);
        
        // Trigger batching if pool is full
        if pool.len() >= self.max_batch_size {
            self.trigger_block_creation().await?;
        }
        
        Ok(())
    }
    
    /// Trigger block creation when conditions are met
    async fn trigger_block_creation(&self) -> Result<(), HotStuffError> {
        let view = self.current_view.lock().await;
        if view.leader == self.node_id {
            let height = self.get_next_height().await;
            let block = self.create_batched_block(height).await?;
            self.propose_block(block).await?;
        }
        Ok(())
    }
    
    // ============ CHAINED CONSENSUS WITH QC EMBEDDING ============
    
    /// Commit using QC and update chain state
    async fn commit_with_qc(&self, qc: QuorumCert, _height: u64) -> Result<(), HotStuffError> {
        let mut chain_state = self.chain_state.lock().await;
        
        // Update high QC if this is higher
        if chain_state.high_qc.as_ref().map(|q| q.height).unwrap_or(0) < qc.height {
            chain_state.high_qc = Some(qc.clone());
        }
        
        // Check commit rule: commit if we have two consecutive QCs
        let prev_qc_clone = chain_state.locked_qc.clone();
        if let Some(prev_qc) = prev_qc_clone {
            if qc.height == prev_qc.height + 1 {
                // Commit the block referenced by prev_qc
                let block_hash = prev_qc.block_hash;
                let prev_height = prev_qc.height;
                drop(chain_state); // Release lock before async call
                
                self.execute_block(&block_hash, prev_height).await?;
                
                // Reacquire lock and update state
                let mut chain_state = self.chain_state.lock().await;
                chain_state.committed_height = prev_height;
                chain_state.b_exec = Some(block_hash);
                chain_state.locked_qc = Some(qc);
                
                info!("✅ Committed block {} at height {} via QC chaining", 
                      block_hash, prev_height);
                return Ok(());
            }
        }
        
        // Update locked QC
        chain_state.locked_qc = Some(qc);
        
        Ok(())
    }
    
    // ============ BLOCK CREATION AND PROPOSAL ============
    
    /// Create a new chained block
    async fn create_chained_block(&self, height: u64) -> Result<Block, HotStuffError> {
        let parent_hash = {
            let chain_state = self.chain_state.lock().await;
            chain_state.high_qc
                .as_ref()
                .map(|qc| qc.block_hash)
                .unwrap_or_else(Hash::zero)
        };
        
        // Get transactions from pool
        let transactions = {
            let mut pool = self.transaction_pool.lock().await;
            let batch_size = self.calculate_optimal_batch_size().await?;
            let pool_len = pool.len();
            let transactions_to_take = std::cmp::min(batch_size, pool_len);
            pool.drain(..transactions_to_take).collect()
        };
        
        let view = self.current_view.lock().await;
        let block = Block::new(parent_hash, transactions, height, view.leader);
        
        Ok(block)
    }
    
    /// Propose a block to the network
    async fn propose_block(&self, block: Block) -> Result<(), HotStuffError> {
        info!("Proposing block {} at height {}", block.hash(), block.height);
        
        // Store block locally
        self.block_store.put_block(&block)?;
        
        // Create proposal
        let proposal = Proposal::new(block.clone());
        
        // Create pipeline stage
        let view = self.current_view.lock().await;
        let block_height = block.height;
        let mut stage = PipelineStage::new(block_height, view.number);
        stage.block = Some(block);
        stage.start_time = Instant::now();
        self.pipeline.insert(block_height, stage);
        
        // Broadcast proposal
        self.broadcast_consensus_message(ConsensusMsg::Proposal(proposal)).await?;
        
        Ok(())
    }
    
    /// Get next height for block creation
    async fn get_next_height(&self) -> u64 {
        let chain_state = self.chain_state.lock().await;
        chain_state.high_qc
            .as_ref()
            .map(|qc| qc.height + 1)
            .unwrap_or(1)
    }
    
    
    // ============ METRICS AND MONITORING ============
    
    /// Update performance metrics
    async fn update_metrics(&self, event: &str, value: f64) {
        let metrics = &self.metrics;
        let metric_event = crate::metrics::MetricEvent::Custom {
            name: event.to_string(),
            value,
            timestamp: Instant::now(),
        };
        
        if let Err(e) = metrics.event_sender().send(metric_event).await {
            warn!("Failed to send metrics event: {}", e);
        }
    }
    
    /// Get current performance statistics
    pub async fn get_performance_stats(&self) -> PerformanceStats {
        let chain_state = self.chain_state.lock().await;
        let view = self.current_view.lock().await;
        let conditions = self.synchrony_detector.get_synchrony_status().await;
        
        PerformanceStats {
            current_height: chain_state.committed_height,
            current_view: view.number,
            is_synchronous: conditions.is_synchronous,
            pipeline_stages: self.pipeline.len(),
            pending_transactions: self.transaction_pool.lock().await.len(),
            fast_path_enabled: self.fast_path_enabled,
        }
    }

    /// Process QC with HotStuff-2 chaining rules
    async fn process_qc_with_chaining(&self, qc: QuorumCert, _height: u64) -> Result<(), HotStuffError> {
        let mut chain_state = self.chain_state.lock().await;
        
        // Update high QC if this is higher
        if chain_state.high_qc.as_ref().map(|q| q.height).unwrap_or(0) < qc.height {
            chain_state.high_qc = Some(qc.clone());
        }
        
        // HotStuff-2 commit rule: commit if we have two consecutive QCs
        let prev_qc_clone = chain_state.locked_qc.clone();
        if let Some(prev_qc) = prev_qc_clone {
            if qc.height == prev_qc.height + 1 {
                // Commit the block referenced by prev_qc
                chain_state.committed_height = prev_qc.height;
                chain_state.b_exec = Some(prev_qc.block_hash);
                
                info!("✅ Committed block {} at height {} via QC chaining", 
                      prev_qc.block_hash, prev_qc.height);
            }
        }
        
        // Update locked QC
        chain_state.locked_qc = Some(qc);
        
        Ok(())
    }

    /// Start view change process
    async fn start_view_change(&self, reason: &str) -> Result<(), HotStuffError> {
        info!("Starting view change: {}", reason);
        
        let mut view = self.current_view.lock().await;
        let next_view = view.number + 1;
        
        // Update to next view
        let leader = self.leader_election.read().get_leader(next_view);
        view.number = next_view;
        view.leader = leader;
        view.start_time = std::time::Instant::now();
        
        // Send NewView message if we're the new leader
        if leader == self.node_id {
            let high_qc = {
                let chain_state = self.chain_state.lock().await;
                chain_state.high_qc.clone()
            };
            if let Some(high_qc) = high_qc {
                drop(view);
                self.send_new_view(next_view, high_qc).await?;
            }
        }
        
        Ok(())
    }

    /// Execute block on state machine
    async fn execute_block(&self, block_hash: &Hash, height: u64) -> Result<(), HotStuffError> {
        info!("Executing block {} at height {}", block_hash, height);
        
        // Get the block from storage
        if let Some(block) = self.block_store.get_block(block_hash)? {
            // Execute block on state machine
            let mut state_machine = self.state_machine.lock().await;
            state_machine.execute_block(&block)?;
            
            info!("✅ Executed block {} with {} transactions", 
                  block_hash, block.transactions.len());
        }
        
        Ok(())
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

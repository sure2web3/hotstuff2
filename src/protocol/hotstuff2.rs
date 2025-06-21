use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use crate::crypto::signature::Signable;
use async_trait::async_trait;
use dashmap::DashMap;
use log::{debug, error, info, warn};
use parking_lot::RwLock;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use tokio::sync::Mutex;

use crate::crypto::{KeyPair, PublicKey, Signature};
use crate::error::HotStuffError;
use crate::message::consensus::{ConsensusMsg, NewView, Timeout, Vote};
use crate::message::network::NetworkMsg;
use crate::network::NetworkClient;
use crate::storage::BlockStore;
use crate::timer::TimeoutManager;
use crate::types::{Block, Hash, Proposal, QuorumCert, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Init,
    PrePrepared,
    Prepared,
    Committed,
    Decided,
}

pub struct HotStuff2<B: BlockStore + ?Sized + 'static> {
    node_id: u64,
    key_pair: KeyPair,
    network_client: Arc<NetworkClient>,
    block_store: Arc<B>,
    timeout_manager: Arc<TimeoutManager>,
    state: Mutex<State>,
    current_height: Mutex<u64>,
    current_round: Mutex<u64>,
    high_qc: Mutex<Option<QuorumCert>>,
    votes: DashMap<Hash, Vec<Vote>>,
    timeouts: DashMap<u64, Vec<Timeout>>, // height -> Timeouts
    leader_election: RwLock<LeaderElection>,
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
    ) -> Arc<Self> {
        // Tolerate f faulty nodes where n = 3f + 1
        let f = (num_nodes - 1) / 3;
        let (message_sender, message_receiver) = mpsc::channel(100);
        // Initialize leader election with node IDs from 0 to num_nodes-1
        let nodes: Vec<_> = (0..num_nodes).collect();
        let leader_election = LeaderElection::new(0, &nodes);

        Arc::new(Self {
            node_id,
            key_pair,
            network_client,
            block_store,
            timeout_manager,
            state: Mutex::new(State::Init),
            current_height: Mutex::new(0),
            current_round: Mutex::new(0),
            high_qc: Mutex::new(None),
            votes: DashMap::new(),
            timeouts: DashMap::new(),
            leader_election: RwLock::new(leader_election),
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
            let height = *this.current_height.lock().await;
            let round = *this.current_round.lock().await;
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
        let block = proposal.block;
        let current_height = *self.current_height.lock().await;

        // Check if the proposal is for the current height
        if block.height != current_height {
            warn!(
                "Received proposal for height {} but current height is {}",
                block.height, current_height
            );
            return Ok(());
        }

        // Check if the sender is the leader for this round
        let current_round = *self.current_round.lock().await;
        let leader = self.leader_election.read().get_leader(current_round);

        if leader != block.proposer_id {
            warn!(
                "Received proposal from {} but leader for round {} is {}",
                block.proposer_id, current_round, leader
            );
            return Ok(());
        }

        // Verify the block's parent exists
        if !self.block_store.has_block(&block.parent_hash)? {
            error!("Block's parent {} not found", block.parent_hash);
            return Err(HotStuffError::Consensus(
                "Block's parent not found".to_string(),
            ));
        }

        // Save the block
        self.block_store.put_block(&block)?;

        // Transition to PrePrepared state
        let mut state = self.state.lock().await;
        *state = State::PrePrepared;
        drop(state);

        // Vote for the block
        self.send_vote(&block).await?;

        Ok(())
    }

    async fn send_vote(&self, block: &Block) -> Result<(), HotStuffError> {
        // Sign the block hash
        let signature = self.sign_hash(&block.hash())?;

        let vote = Vote {
            block_hash: block.hash(),
            height: block.height,
            sender_id: self.node_id,
            signature: signature.as_bytes().to_vec(),
        };

        // Broadcast the vote
        self.broadcast_consensus_message(ConsensusMsg::Vote(vote))
            .await?;

        Ok(())
    }

    async fn handle_vote(&self, vote: Vote) -> Result<(), HotStuffError> {
        let current_height = *self.current_height.lock().await;

        // Check if the vote is for the current height
        if vote.height != current_height {
            warn!(
                "Received vote for height {} but current height is {}",
                vote.height, current_height
            );
            return Ok(());
        }

        // Verify the signature
        // TODO: Get the public key for the voter
        let public_key = PublicKey([0u8; 32]); // Dummy public key
        if !public_key.verify(&vote.block_hash.bytes(), &vote.signature)? {
            error!("Invalid signature on vote from {}", vote.sender_id);
            return Err(HotStuffError::Consensus(
                "Invalid signature on vote".to_string(),
            ));
        }

        // Add the vote to the collection
        let mut votes = self.votes.entry(vote.block_hash).or_insert(Vec::new());
        votes.push(vote.clone());

        // Check if we have enough votes to form a quorum certificate
        if votes.len() as u64 >= self.num_nodes - self.f {
            // Create a quorum certificate
            let signatures = votes
                .iter()
                .map(|v| (v.sender_id, v.signature.clone()))
                .collect();
            let qc = QuorumCert::new(vote.block_hash, vote.height, signatures);

            // Save the quorum certificate
            // TODO: Implement storage for quorum certificates

            // Update high_qc if this QC is higher
            let mut high_qc = self.high_qc.lock().await;
            if high_qc.as_ref().map(|h| h.height).unwrap_or(0) < qc.height {
                *high_qc = Some(qc.clone());
            }
            drop(high_qc);

            // Transition to Prepared state
            let mut state = self.state.lock().await;
            *state = State::Prepared;
            drop(state);

            // Broadcast the new view if we're the leader for the next round
            let current_round = *self.current_round.lock().await;
            let next_round = current_round + 1;
            let leader = self.leader_election.read().get_leader(next_round);

            if leader == self.node_id {
                self.send_new_view(next_round, qc).await?;
            }
        }

        Ok(())
    }

    async fn send_new_view(&self, round: u64, high_qc: QuorumCert) -> Result<(), HotStuffError> {
        let current_height = *self.current_height.lock().await;

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
        let current_height = *self.current_height.lock().await;

        // Check if the timeout is for the current height
        if timeout.height != current_height {
            warn!(
                "Received timeout for height {} but current height is {}",
                timeout.height, current_height
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
        timeouts.push(timeout.clone());

        // Check if we have enough timeouts to trigger a new view
        if timeouts.len() as u64 >= self.num_nodes - self.f {
            // Get the highest QC from the timeouts
            let highest_qc = timeouts
                .iter()
                .map(|t| &t.high_qc)
                .max_by_key(|qc| qc.height);

            if let Some(qc) = highest_qc {
                // Update high_qc if this QC is higher
                let mut high_qc = self.high_qc.lock().await;
                if high_qc.as_ref().map(|h| h.height).unwrap_or(0) < qc.height {
                    *high_qc = Some(qc.clone());
                }
                drop(high_qc);

                // Increment the round
                let mut current_round = self.current_round.lock().await;
                *current_round += 1;
                let new_round = *current_round;
                drop(current_round);

                // Cancel the current timeout
                self.timeout_manager
                    .cancel_timeout(current_height, new_round - 1)
                    .await?;

                // Start a new timeout for the new round
                self.timeout_manager
                    .start_timeout(current_height, new_round)
                    .await?;

                // If we're the leader for the new round, send a NewView
                let leader = self.leader_election.read().get_leader(new_round);
                if leader == self.node_id {
                    self.send_new_view(new_round, qc.clone()).await?;
                }
            }
        }

        Ok(())
    }

    async fn handle_new_view(&self, new_view: NewView) -> Result<(), HotStuffError> {
        let current_height = *self.current_height.lock().await;

        // Check if the NewView is for the current height
        if new_view.new_view_for_height != current_height {
            warn!(
                "Received NewView for height {} but current height is {}",
                new_view.new_view_for_height, current_height
            );
            return Ok(());
        }

        // Check if the sender is the leader for the new round
        let leader = self
            .leader_election
            .read()
            .get_leader(new_view.new_view_for_round);
        if leader != new_view.sender_id {
            warn!(
                "Received NewView from {} but leader for round {} is {}",
                new_view.sender_id, new_view.new_view_for_round, leader
            );
            return Ok(());
        }

        // Verify the timeout certificates
        // TODO: Verify the timeout certificates

        // Update our round
        let mut current_round = self.current_round.lock().await;
        if *current_round < new_view.new_view_for_round {
            *current_round = new_view.new_view_for_round;
        }
        drop(current_round);

        // If there's a new leader block, process it
        if let Some(block) = new_view.new_leader_block {
            // Save the block
            self.block_store.put_block(&block)?;

            // Transition to PrePrepared state
            let mut state = self.state.lock().await;
            *state = State::PrePrepared;
            drop(state);

            // Vote for the block
            self.send_vote(&block).await?;
        } else {
            // No new block, but we should still proceed with the new view
            // TODO: Handle this case
        }

        Ok(())
    }

    async fn broadcast_consensus_message(&self, msg: ConsensusMsg) -> Result<(), HotStuffError> {
        // Convert the consensus message to a network message
        let network_msg = NetworkMsg::Consensus(msg);

        // Use a public method to get peer IDs
        for peer_id in self.network_client.peer_ids() {
            self.network_client
                .send(*peer_id, network_msg.clone())
                .await?;
        }

        Ok(())
    }

    fn sign_hash(&self, hash: &Hash) -> Result<Signature, HotStuffError> {
        // In a real implementation, use proper cryptographic signing
        // This is a placeholder
        Ok(Signature::new(hash.as_bytes().to_vec()))
    }
}

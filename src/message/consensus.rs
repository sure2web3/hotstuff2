use serde::{Deserialize, Serialize};

use crate::crypto::bls_threshold::BlsSignature;
use crate::types::{Block, Hash, Proposal, QuorumCert};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum ConsensusMsg {
    Proposal(Proposal),
    Vote(Vote),
    Timeout(Timeout),
    NewView(NewView),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Vote {
    pub view: u64,
    pub height: u64,
    pub block_hash: Hash,
    pub sender_id: u64,
    pub signature: Vec<u8>,
    pub partial_signature: Option<BlsSignature>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Timeout {
    pub height: u64,
    pub round: u64,
    pub sender_id: u64,
    pub high_qc: QuorumCert,
    pub signature: Vec<u8>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct NewView {
    pub new_view_for_height: u64,
    pub new_view_for_round: u64,
    pub sender_id: u64,
    pub timeout_certs: Vec<Timeout>,
    pub new_leader_block: Option<Block>,
}

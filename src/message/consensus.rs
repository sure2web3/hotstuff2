use serde::{Deserialize, Serialize};

use crate::types::{Hash, QuorumCert, Signature as TypesSignature};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum ConsensusMsg {
    Proposal(ConsensusProposal),
    Vote(Vote),
    Timeout(Timeout),
    NewView(NewView),
    FastCommit(FastCommit),
}

/// Fast commit message for HotStuff-2 optimistic responsiveness
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FastCommit {
    pub block_hash: Hash,
    pub height: u64,
    pub view: u64,
    pub signature: TypesSignature,
    pub node_id: u64,
}

/// Vote message for consensus
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Vote {
    pub view: u64,
    pub block_hash: Hash,
    pub node_id: u64,
    pub signature: TypesSignature,
}

/// Timeout message for view changes
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Timeout {
    pub view: u64,
    pub node_id: u64,
    pub high_qc: Option<QuorumCert>,
    pub signature: TypesSignature,
}

/// New view message for leader changes
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct NewView {
    pub view: u64,
    pub node_id: u64,
    pub timeout_certs: Vec<Timeout>,
    pub high_qc: Option<QuorumCert>,
}

/// Proposal message for new blocks
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ConsensusProposal {
    pub block_hash: Hash,
    pub view: u64,
    pub node_id: u64,
    pub qc: Option<QuorumCert>,
}

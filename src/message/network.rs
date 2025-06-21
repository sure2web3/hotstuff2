use serde::{Deserialize, Serialize};

use crate::message::consensus::ConsensusMsg;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum NetworkMsg {
    Consensus(ConsensusMsg),
    PeerDiscovery(PeerDiscoveryMsg),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum PeerDiscoveryMsg {
    Hello(PeerAddr),
    Peers(Vec<PeerAddr>),
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq, Hash)]
pub struct PeerAddr {
    pub node_id: u64,
    pub address: String,
}

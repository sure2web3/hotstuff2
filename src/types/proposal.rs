use serde::{Deserialize, Serialize};

use crate::types::{Block, Timestamp};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Proposal {
    pub block: Block,
    pub timestamp: Timestamp,
}

impl Proposal {
    pub fn new(block: Block) -> Self {
        Self {
            block,
            timestamp: Timestamp::now(),
        }
    }
}

use std::collections::HashMap;

use crate::message::network::PeerAddr;

#[derive(Clone, Debug)]
pub struct NodeConfig {
    pub node_id: u64,
    pub listen_addr: String,
    pub peers: HashMap<u64, String>,
    pub data_dir: String,
    pub base_timeout: u64, // Milliseconds
    pub timeout_multiplier: f64,
    pub max_mempool_size: usize,
    pub metrics_port: Option<u16>,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            node_id: 0,
            listen_addr: "127.0.0.1:8000".to_string(),
            peers: HashMap::new(),
            data_dir: "./data".to_string(),
            base_timeout: 1000, // 1 second
            timeout_multiplier: 1.5,
            max_mempool_size: 10000,
            metrics_port: None,
        }
    }
}

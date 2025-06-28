use std::sync::Arc;
use std::time::Duration;

use log::{debug, error, info};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::config::HotStuffConfig;
use crate::consensus::state_machine::KVStateMachine;
use crate::crypto::KeyPair;
use crate::error::HotStuffError;
use crate::message::consensus::ConsensusMsg;
use crate::message::network::{NetworkMsg, PeerAddr};
use crate::network::{NetworkClient, NetworkServer};
use crate::node::NodeConfig;
use crate::protocol::hotstuff2::HotStuff2;
use crate::storage::{block_store::MemoryBlockStore, BlockStore, Mempool, RocksDBStore};
use crate::timer::TimeoutManager;
use crate::types::Proposal;

pub struct Node {
    config: NodeConfig,
    key_pair: KeyPair,
    network_client: Arc<NetworkClient>,
    network_server: Option<Arc<NetworkServer>>,
    block_store: Arc<dyn BlockStore>,
    mempool: Arc<Mempool>,
    hotstuff: Option<Arc<HotStuff2<dyn BlockStore>>>,
    timeout_manager: Arc<TimeoutManager>,
    running: std::sync::Mutex<bool>,
    server_handle: Option<JoinHandle<Result<(), HotStuffError>>>,
    message_sender: mpsc::Sender<(u64, NetworkMsg)>,
}

impl Node {
    pub fn new(config: NodeConfig) -> Self {
        // Generate key pair
        let mut rng = rand::rng();
        let key_pair = KeyPair::generate(&mut rng);

        // Convert peers vec to PeerAddr structs
        let peers = config
            .peers
            .iter()
            .map(|(id, addr)| {
                (
                    *id,
                    PeerAddr {
                        node_id: *id,
                        address: addr.clone(),
                    },
                )
            })
            .collect();

        // Create network client
        let network_client = Arc::new(NetworkClient::new(config.node_id, peers));

        // Create message channel
        let (message_sender, _message_receiver) = mpsc::channel(100);

        // Create block store
        let block_store: Arc<dyn BlockStore> = match std::fs::create_dir_all(&config.data_dir) {
            Ok(_) => {
                let path = format!("{}/blocks", config.data_dir);
                match RocksDBStore::open(&path) {
                    Ok(store) => Arc::new(store),
                    Err(e) => {
                        error!("Failed to open RocksDB store: {}, using memory store", e);
                        Arc::new(MemoryBlockStore::new())
                    }
                }
            }
            Err(e) => {
                error!("Failed to create data directory: {}, using memory store", e);
                Arc::new(MemoryBlockStore::new())
            }
        };

        // Create mempool
        let mempool = Arc::new(Mempool::new(config.max_mempool_size));

        // Create timeout manager
        let timeout_manager = TimeoutManager::new(
            Duration::from_millis(config.base_timeout),
            config.timeout_multiplier,
        );

        Self {
            config,
            key_pair,
            network_client,
            network_server: None,
            block_store,
            mempool,
            hotstuff: None,
            timeout_manager,
            running: std::sync::Mutex::new(false),
            server_handle: None,
            message_sender,
        }
    }

    pub fn node_id(&self) -> u64 {
        self.config.node_id
    }

    pub async fn start(&mut self) -> Result<(), HotStuffError> {
        let mut running = self.running.lock().unwrap();

        if *running {
            return Err(HotStuffError::AlreadyStarted);
        }

        // Start the network server
        let network_server = Arc::new(
            NetworkServer::new(
                &self.config.listen_addr,
                self.config.node_id,
                self.message_sender.clone(),
            )
            .await?,
        );

        let server_handle = {
            let network_server = Arc::clone(&network_server);
            tokio::spawn(async move { network_server.start().await })
        };

        self.network_server = Some(network_server);
        self.server_handle = Some(server_handle);

        // Start the timeout manager
        self.timeout_manager.start();

        // Create configuration for HotStuff-2
        let hotstuff_config = HotStuffConfig {
            node_id: self.config.node_id,
            listen_addr: self.config.listen_addr.clone(),
            peers: self.config.peers.iter().map(|(id, addr)| crate::config::PeerConfig {
                node_id: *id,
                address: addr.clone(),
                public_key: None, // TODO: Add real public keys
            }).collect(),
            consensus: crate::config::ConsensusConfig::default(),
            network: crate::config::NetworkConfig::default(),
            storage: crate::config::StorageConfig::default(),
            metrics: crate::config::MetricsConfig::default(),
            crypto: crate::config::CryptoConfig::default(),
            logging: crate::config::LoggingConfig::default(),
        };
        
        // Create state machine
        let state_machine = Arc::new(Mutex::new(KVStateMachine::new()));

        // Initialize and start HotStuff-2 protocol
        let hotstuff = HotStuff2::new(
            self.config.node_id,
            self.key_pair.clone(),
            self.network_client.clone(),
            self.block_store.clone(),
            self.timeout_manager.clone(),
            self.config.peers.len() as u64 + 1, // Total nodes including self
            hotstuff_config,
            state_machine,
        );

        hotstuff.start();
        self.hotstuff = Some(hotstuff);

        // Start message handler
        self.start_message_handler().await?;

        *running = true;
        info!("Node {} started successfully", self.config.node_id);

        Ok(())
    }

    async fn start_message_handler(&self) -> Result<(), HotStuffError> {
        let (_tx, mut rx) = mpsc::channel::<(u64, NetworkMsg)>(100);

        // Clone necessary fields
        let hotstuff = self.hotstuff.as_ref().unwrap().get_message_sender();
        let node_id = self.config.node_id;

        tokio::spawn(async move {
            while let Some((sender_id, msg)) = rx.recv().await {
                match msg {
                    NetworkMsg::Consensus(consensus_msg) => {
                        if sender_id != node_id {
                            // Avoid processing our own messages
                            if let Err(e) = hotstuff.send(consensus_msg).await {
                                error!("Failed to send message to HotStuff: {}", e);
                            }
                        }
                    }
                    NetworkMsg::PeerDiscovery(discovery_msg) => {
                        // Handle peer discovery
                        // TODO: Implement peer discovery logic
                        debug!("Received peer discovery message: {:?}", discovery_msg);
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), HotStuffError> {
        let mut running = self.running.lock().unwrap();

        if !*running {
            return Err(HotStuffError::NotRunning);
        }

        // Stop the timeout manager
        self.timeout_manager.shutdown().await;

        // Stop the network server
        if let Some(handle) = self.server_handle.take() {
            handle.abort();
            match handle.await {
                Ok(_) => info!("Network server stopped"),
                Err(e) => error!("Error stopping network server: {}", e),
            }
        }

        *running = false;
        info!("Node {} stopped successfully", self.config.node_id);

        Ok(())
    }

    pub async fn propose(&self, block: crate::types::Block) -> Result<(), HotStuffError> {
        let hotstuff = self.hotstuff.as_ref().ok_or(HotStuffError::NotRunning)?;

        let proposal = Proposal::new(block);
        let sender = hotstuff.get_message_sender();

        sender
            .send(ConsensusMsg::Proposal(proposal))
            .await
            .map_err(|e| HotStuffError::Network(format!("Send failed: {e}")))?;

        Ok(())
    }

    pub async fn committed_block(&self) -> Option<crate::types::Block> {
        // Get the latest committed block from the block store
        if let Ok(block) = self.block_store.get_latest_committed_block() {
            Some(block)
        } else {
            None
        }
    }
}

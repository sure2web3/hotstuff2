use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use futures::future::BoxFuture;
use futures::FutureExt;
use log::{debug, error, info};
use parking_lot::Mutex;
use tokio::io::{split, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::instrument;

use crate::error::HotStuffError;
use crate::message::network::PeerDiscoveryMsg;
use crate::message::network::{NetworkMsg, PeerAddr};

type ConnectionPool = DashMap<u64, mpsc::Sender<NetworkMsg>>;

pub struct NetworkClient {
    node_id: u64,
    peers: HashMap<u64, PeerAddr>,
    connections: ConnectionPool,
    retry_delay: Duration,
    max_retries: u32,
}

impl NetworkClient {
    pub fn new(node_id: u64, peers: HashMap<u64, PeerAddr>) -> Self {
        Self {
            node_id,
            peers,
            connections: DashMap::new(),
            retry_delay: Duration::from_secs(1),
            max_retries: 3,
        }
    }

    pub fn peer_ids(&self) -> impl Iterator<Item = &u64> {
        self.peers.keys()
    }

    #[instrument(skip(self, msg), fields(recipient = recipient_id))]
    pub async fn send(&self, recipient_id: u64, msg: NetworkMsg) -> Result<(), HotStuffError> {
        // Get or establish a connection to the recipient
        let sender = self.get_or_connect(recipient_id).await?;

        // Send the message
        if let Err(e) = sender.send(msg).await {
            error!("Failed to send message to {}: {}", recipient_id, e);
            // Remove the connection from the pool
            self.connections.remove(&recipient_id);
            return Err(HotStuffError::Network(format!(
                "Failed to send message to {}: {}",
                recipient_id, e
            )));
        }

        Ok(())
    }

    async fn get_or_connect(
        &self,
        peer_id: u64,
    ) -> Result<mpsc::Sender<NetworkMsg>, HotStuffError> {
        // Check if we already have a connection
        if let Some(sender) = self.connections.get(&peer_id) {
            return Ok(sender.clone());
        }

        // Establish a new connection
        let peer_addr = self
            .peers
            .get(&peer_id)
            .ok_or_else(|| HotStuffError::Network(format!("Unknown peer ID: {}", peer_id)))?;

        let (sender, receiver) = mpsc::channel(100);
        let peer_addr_clone = peer_addr.clone();
        let node_id = self.node_id;

        // Spawn a task to handle the connection
        tokio::spawn(async move {
            match Self::connect_and_handle(peer_id, peer_addr_clone, node_id, receiver).await {
                Ok(_) => info!("Connection to peer {} closed normally", peer_id),
                Err(e) => error!("Connection to peer {} failed: {}", peer_id, e),
            }
        });

        // Add the connection to the pool
        self.connections.insert(peer_id, sender.clone());

        Ok(sender)
    }

    async fn connect_and_handle(
        peer_id: u64,
        peer_addr: PeerAddr,
        node_id: u64,
        mut receiver: mpsc::Receiver<NetworkMsg>,
    ) -> Result<(), HotStuffError> {
        // Connect to the peer
        let addr = &peer_addr.address;
        info!("Connecting to peer {} at {}", peer_id, addr);

        let stream = timeout(Duration::from_secs(5), TcpStream::connect(addr))
            .await
            .map_err(|_| HotStuffError::Network(format!("Connection timeout to {}", addr)))?
            .map_err(|e| HotStuffError::Network(format!("Failed to connect to {}: {}", addr, e)))?;

        info!("Connected to peer {}", peer_id);

        // Split the stream to get WriteHalf
        let (_read_half, mut write_half) = split(stream);

        // Send a hello message
        let hello_msg = NetworkMsg::PeerDiscovery(PeerDiscoveryMsg::Hello(PeerAddr {
            node_id,
            address: "unknown".to_string(), // TODO: Get our own address
        }));

        // Serialize and send the hello message
        let serialized = bincode::serialize(&hello_msg)?;
        super::transport::write_message(&mut write_half, &serialized).await?;

        // Handle outgoing messages
        while let Some(msg) = receiver.recv().await {
            let serialized = bincode::serialize(&msg)?;
            if let Err(e) = super::transport::write_message(&mut write_half, &serialized).await {
                error!("Failed to send message to {}: {}", peer_id, e);
                return Err(e);
            }
        }

        Ok(())
    }
}

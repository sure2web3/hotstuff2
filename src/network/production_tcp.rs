// Production TCP-based P2P networking for HotStuff-2
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock, Mutex};
use tokio::time::{interval, timeout};

use crate::error::HotStuffError;
use crate::message::network::NetworkMsg;

/// Production P2P network implementation with real TCP connections
pub struct ProductionP2PNetwork {
    /// This node's ID
    node_id: u64,
    /// Address this node listens on
    listen_addr: SocketAddr,
    /// Active peer connections
    peers: Arc<RwLock<HashMap<u64, PeerConnection>>>,
    /// Incoming message channel
    message_sender: mpsc::UnboundedSender<(u64, NetworkMsg)>,
    /// Network configuration
    config: NetworkConfig,
    /// Connection management
    connection_manager: Arc<ConnectionManager>,
    /// Network statistics
    stats: Arc<Mutex<NetworkStats>>,
}

/// Individual peer connection
#[derive(Debug)]
pub struct PeerConnection {
    pub peer_id: u64,
    pub peer_addr: SocketAddr,
    pub message_sender: mpsc::UnboundedSender<WireMessage>,
    pub last_heartbeat: Instant,
    pub connected_at: Instant,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
}

/// Network configuration parameters
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub connection_timeout: Duration,
    pub heartbeat_interval: Duration,
    pub max_message_size: usize,
    pub reconnect_attempts: u32,
    pub reconnect_delay: Duration,
    pub message_queue_size: usize,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            connection_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(30),
            max_message_size: 1024 * 1024, // 1MB
            reconnect_attempts: 5,
            reconnect_delay: Duration::from_secs(5),
            message_queue_size: 1000,
        }
    }
}

/// Wire protocol message format for network transmission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireMessage {
    pub message_id: u64,
    pub from: u64,
    pub to: Option<u64>, // None for broadcast
    pub message_type: WireMessageType,
    pub payload: Vec<u8>,
    pub timestamp: u64,
    pub signature: Option<Vec<u8>>,
}

/// Wire message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WireMessageType {
    Consensus,
    Heartbeat,
    PeerDiscovery,
    NetworkInfo,
}

/// Connection management for handling peer lifecycle
pub struct ConnectionManager {
    node_id: u64,
    peers: Arc<RwLock<HashMap<u64, PeerConnection>>>,
    config: NetworkConfig,
    stats: Arc<Mutex<NetworkStats>>,
}

/// Network statistics tracking
#[derive(Debug, Clone, Default)]
pub struct NetworkStats {
    pub total_connections: u64,
    pub active_connections: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub connection_failures: u64,
    pub heartbeats_sent: u64,
    pub heartbeats_received: u64,
}

impl ProductionP2PNetwork {
    /// Create new production P2P network
    pub fn new(
        node_id: u64,
        listen_addr: SocketAddr,
        config: NetworkConfig,
    ) -> (Self, mpsc::UnboundedReceiver<(u64, NetworkMsg)>) {
        let (message_sender, message_receiver) = mpsc::unbounded_channel();
        let peers = Arc::new(RwLock::new(HashMap::new()));
        let stats = Arc::new(Mutex::new(NetworkStats::default()));
        
        let connection_manager = Arc::new(ConnectionManager {
            node_id,
            peers: Arc::clone(&peers),
            config: config.clone(),
            stats: Arc::clone(&stats),
        });

        let network = Self {
            node_id,
            listen_addr,
            peers,
            message_sender,
            config,
            connection_manager,
            stats,
        };

        (network, message_receiver)
    }

    /// Start the network server and begin accepting connections
    pub async fn start(&self) -> Result<(), HotStuffError> {
        info!("Starting P2P network server on {} for node {}", self.listen_addr, self.node_id);

        // Start TCP listener
        let listener = TcpListener::bind(&self.listen_addr).await
            .map_err(|e| HotStuffError::Network(format!("Failed to bind to {}: {}", self.listen_addr, e)))?;

        // Start accepting connections
        let connection_manager = Arc::clone(&self.connection_manager);
        let message_sender = self.message_sender.clone();
        
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        info!("Accepted connection from {}", addr);
                        let manager = Arc::clone(&connection_manager);
                        let sender = message_sender.clone();
                        
                        tokio::spawn(async move {
                            if let Err(e) = manager.handle_incoming_connection(stream, addr, sender).await {
                                error!("Failed to handle incoming connection from {}: {}", addr, e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to accept connection: {}", e);
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        });

        // Start heartbeat mechanism
        self.start_heartbeat_task().await;

        info!("P2P network started successfully for node {}", self.node_id);
        Ok(())
    }

    /// Connect to a peer node
    pub async fn connect_to_peer(&self, peer_id: u64, peer_addr: SocketAddr) -> Result<(), HotStuffError> {
        info!("Connecting to peer {} at {}", peer_id, peer_addr);

        // Check if already connected
        {
            let peers = self.peers.read().await;
            if peers.contains_key(&peer_id) {
                debug!("Already connected to peer {}", peer_id);
                return Ok(());
            }
        }

        // Establish TCP connection
        let stream = timeout(self.config.connection_timeout, TcpStream::connect(peer_addr)).await
            .map_err(|_| HotStuffError::Network(format!("Connection to {} timed out", peer_addr)))?
            .map_err(|e| HotStuffError::Network(format!("Failed to connect to {}: {}", peer_addr, e)))?;

        info!("TCP connection established to peer {} at {}", peer_id, peer_addr);

        // Handle the connection
        self.connection_manager.handle_outgoing_connection(stream, peer_id, peer_addr, self.message_sender.clone()).await
    }

    /// Send message to specific peer
    pub async fn send_to_peer(&self, peer_id: u64, message: NetworkMsg) -> Result<(), HotStuffError> {
        let peers = self.peers.read().await;
        
        if let Some(peer) = peers.get(&peer_id) {
            let wire_message = self.create_wire_message(Some(peer_id), message).await?;
            
            if let Err(_) = peer.message_sender.send(wire_message) {
                warn!("Failed to send message to peer {}: channel closed", peer_id);
                return Err(HotStuffError::Network(format!("Peer {} disconnected", peer_id)));
            }
            
            // Update statistics
            let mut stats = self.stats.lock().await;
            stats.messages_sent += 1;
            
            Ok(())
        } else {
            Err(HotStuffError::Network(format!("Peer {} not connected", peer_id)))
        }
    }

    /// Broadcast message to all connected peers
    pub async fn broadcast(&self, message: NetworkMsg) -> Result<(), HotStuffError> {
        let peers = self.peers.read().await;
        let wire_message = self.create_wire_message(None, message).await?;
        
        let mut sent_count = 0;
        let mut failed_peers = Vec::new();

        for (peer_id, peer) in peers.iter() {
            if let Err(_) = peer.message_sender.send(wire_message.clone()) {
                warn!("Failed to broadcast to peer {}: channel closed", peer_id);
                failed_peers.push(*peer_id);
            } else {
                sent_count += 1;
            }
        }

        // Update statistics
        let mut stats = self.stats.lock().await;
        stats.messages_sent += sent_count;

        // Remove failed peers (will be handled by connection manager)
        drop(peers);
        for peer_id in failed_peers {
            self.disconnect_peer(peer_id).await;
        }

        info!("Broadcast message sent to {} peers", sent_count);
        Ok(())
    }

    /// Disconnect from a peer
    pub async fn disconnect_peer(&self, peer_id: u64) {
        let mut peers = self.peers.write().await;
        if let Some(_) = peers.remove(&peer_id) {
            info!("Disconnected from peer {}", peer_id);
            
            let mut stats = self.stats.lock().await;
            stats.active_connections = stats.active_connections.saturating_sub(1);
        }
    }

    /// Get list of connected peers
    pub async fn get_connected_peers(&self) -> Vec<u64> {
        let peers = self.peers.read().await;
        peers.keys().cloned().collect()
    }

    /// Get network statistics
    pub async fn get_stats(&self) -> NetworkStats {
        self.stats.lock().await.clone()
    }

    /// Create wire message from network message
    async fn create_wire_message(&self, to: Option<u64>, message: NetworkMsg) -> Result<WireMessage, HotStuffError> {
        let payload = bincode::serialize(&message)
            .map_err(|e| HotStuffError::Network(format!("Failed to serialize message: {}", e)))?;

        if payload.len() > self.config.max_message_size {
            return Err(HotStuffError::Network(format!("Message too large: {} bytes", payload.len())));
        }

        let message_id = rand::random::<u64>();

        Ok(WireMessage {
            message_id,
            from: self.node_id,
            to,
            message_type: WireMessageType::Consensus,
            payload,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            signature: None, // TODO: Add message signing
        })
    }

    /// Start heartbeat task for connection health monitoring
    async fn start_heartbeat_task(&self) {
        let peers = Arc::clone(&self.peers);
        let node_id = self.node_id;
        let heartbeat_interval = self.config.heartbeat_interval;
        let stats = Arc::clone(&self.stats);

        tokio::spawn(async move {
            let mut interval = interval(heartbeat_interval);
            
            loop {
                interval.tick().await;
                
                let connected_peers: Vec<u64> = {
                    let peers_guard = peers.read().await;
                    peers_guard.keys().cloned().collect()
                };

                for &peer_id in &connected_peers {
                    let heartbeat_msg = WireMessage {
                        message_id: rand::random(),
                        from: node_id,
                        to: Some(peer_id),
                        message_type: WireMessageType::Heartbeat,
                        payload: Vec::new(),
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                        signature: None,
                    };

                    let sent = {
                        let peers_guard = peers.read().await;
                        if let Some(peer) = peers_guard.get(&peer_id) {
                            peer.message_sender.send(heartbeat_msg).is_ok()
                        } else {
                            false
                        }
                    };

                    if sent {
                        let mut stats_guard = stats.lock().await;
                        stats_guard.heartbeats_sent += 1;
                    } else {
                        warn!("Failed to send heartbeat to peer {}", peer_id);
                    }
                }

                debug!("Sent heartbeats to {} peers", connected_peers.len());
            }
        });
    }
}

impl ConnectionManager {
    /// Handle incoming TCP connection
    async fn handle_incoming_connection(
        &self,
        stream: TcpStream,
        addr: SocketAddr,
        message_sender: mpsc::UnboundedSender<(u64, NetworkMsg)>,
    ) -> Result<(), HotStuffError> {
        let peer_addr = stream.peer_addr()
            .map_err(|e| HotStuffError::Network(format!("Failed to get peer address: {}", e)))?;

        info!("Handling incoming connection from {}", peer_addr);

        // For incoming connections, we need to perform handshake to get peer_id
        // For now, we'll use a simple approach - TODO: implement proper handshake
        let peer_id = addr.port() as u64; // Temporary peer ID assignment

        self.setup_connection(stream, peer_id, peer_addr, message_sender).await
    }

    /// Handle outgoing TCP connection
    async fn handle_outgoing_connection(
        &self,
        stream: TcpStream,
        peer_id: u64,
        peer_addr: SocketAddr,
        message_sender: mpsc::UnboundedSender<(u64, NetworkMsg)>,
    ) -> Result<(), HotStuffError> {
        info!("Handling outgoing connection to peer {} at {}", peer_id, peer_addr);
        self.setup_connection(stream, peer_id, peer_addr, message_sender).await
    }

    /// Setup bidirectional communication for a connection
    async fn setup_connection(
        &self,
        stream: TcpStream,
        peer_id: u64,
        peer_addr: SocketAddr,
        message_sender: mpsc::UnboundedSender<(u64, NetworkMsg)>,
    ) -> Result<(), HotStuffError> {
        let (reader, writer) = stream.into_split();
        let (tx, rx) = mpsc::unbounded_channel();

        // Create peer connection record
        let connection = PeerConnection {
            peer_id,
            peer_addr,
            message_sender: tx,
            last_heartbeat: Instant::now(),
            connected_at: Instant::now(),
            bytes_sent: 0,
            bytes_received: 0,
            messages_sent: 0,
            messages_received: 0,
        };

        // Store the connection
        {
            let mut peers = self.peers.write().await;
            peers.insert(peer_id, connection);
            
            let mut stats = self.stats.lock().await;
            stats.total_connections += 1;
            stats.active_connections += 1;
        }

        // Start writer task
        let writer_stats = Arc::clone(&self.stats);
        tokio::spawn(async move {
            Self::handle_writer(writer, rx, writer_stats).await;
        });

        // Start reader task
        let reader_peers = Arc::clone(&self.peers);
        let reader_stats = Arc::clone(&self.stats);
        tokio::spawn(async move {
            if let Err(e) = Self::handle_reader(reader, peer_id, message_sender, reader_peers, reader_stats).await {
                error!("Reader task failed for peer {}: {}", peer_id, e);
            }
        });

        info!("Connection established with peer {} at {}", peer_id, peer_addr);
        Ok(())
    }

    /// Handle writing messages to peer
    async fn handle_writer(
        mut writer: tokio::net::tcp::OwnedWriteHalf,
        mut rx: mpsc::UnboundedReceiver<WireMessage>,
        stats: Arc<Mutex<NetworkStats>>,
    ) {
        while let Some(message) = rx.recv().await {
            match bincode::serialize(&message) {
                Ok(data) => {
                    let frame = format!("{}\n", hex::encode(&data));
                    
                    if let Err(e) = writer.write_all(frame.as_bytes()).await {
                        error!("Failed to write to peer: {}", e);
                        break;
                    }

                    let mut stats_guard = stats.lock().await;
                    stats_guard.bytes_sent += frame.len() as u64;
                }
                Err(e) => {
                    error!("Failed to serialize message: {}", e);
                }
            }
        }
    }

    /// Handle reading messages from peer
    async fn handle_reader(
        reader: tokio::net::tcp::OwnedReadHalf,
        peer_id: u64,
        message_sender: mpsc::UnboundedSender<(u64, NetworkMsg)>,
        peers: Arc<RwLock<HashMap<u64, PeerConnection>>>,
        stats: Arc<Mutex<NetworkStats>>,
    ) -> Result<(), HotStuffError> {
        let mut buf_reader = BufReader::new(reader);
        let mut line = String::new();

        loop {
            line.clear();
            
            match buf_reader.read_line(&mut line).await {
                Ok(0) => {
                    info!("Connection closed by peer {}", peer_id);
                    break;
                }
                Ok(bytes_read) => {
                    let mut stats_guard = stats.lock().await;
                    stats_guard.bytes_received += bytes_read as u64;
                    drop(stats_guard);

                    // Process the received line
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        if let Err(e) = Self::process_received_message(
                            trimmed, peer_id, &message_sender, &peers, &stats
                        ).await {
                            warn!("Failed to process message from peer {}: {}", peer_id, e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to read from peer {}: {}", peer_id, e);
                    break;
                }
            }
        }

        // Clean up connection
        {
            let mut peers_guard = peers.write().await;
            peers_guard.remove(&peer_id);
            
            let mut stats_guard = stats.lock().await;
            stats_guard.active_connections = stats_guard.active_connections.saturating_sub(1);
        }

        Ok(())
    }

    /// Process received message from peer
    async fn process_received_message(
        hex_data: &str,
        peer_id: u64,
        message_sender: &mpsc::UnboundedSender<(u64, NetworkMsg)>,
        peers: &Arc<RwLock<HashMap<u64, PeerConnection>>>,
        stats: &Arc<Mutex<NetworkStats>>,
    ) -> Result<(), HotStuffError> {
        // Decode hex data
        let data = hex::decode(hex_data)
            .map_err(|e| HotStuffError::Network(format!("Failed to decode hex: {}", e)))?;

        // Deserialize wire message
        let wire_message: WireMessage = bincode::deserialize(&data)
            .map_err(|e| HotStuffError::Network(format!("Failed to deserialize message: {}", e)))?;

        // Update statistics
        {
            let mut stats_guard = stats.lock().await;
            stats_guard.messages_received += 1;
            
            if matches!(wire_message.message_type, WireMessageType::Heartbeat) {
                stats_guard.heartbeats_received += 1;
            }
        }

        // Update peer heartbeat
        {
            let mut peers_guard = peers.write().await;
            if let Some(peer) = peers_guard.get_mut(&peer_id) {
                peer.last_heartbeat = Instant::now();
                peer.messages_received += 1;
            }
        }

        // Handle different message types
        match wire_message.message_type {
            WireMessageType::Consensus => {
                // Deserialize consensus message
                let network_msg: NetworkMsg = bincode::deserialize(&wire_message.payload)
                    .map_err(|e| HotStuffError::Network(format!("Failed to deserialize consensus message: {}", e)))?;

                // Forward to consensus layer
                if let Err(_) = message_sender.send((peer_id, network_msg)) {
                    error!("Failed to forward message to consensus layer");
                }
            }
            WireMessageType::Heartbeat => {
                debug!("Received heartbeat from peer {}", peer_id);
                // Heartbeat is already processed above
            }
            WireMessageType::PeerDiscovery => {
                // TODO: Implement peer discovery protocol
                debug!("Received peer discovery message from {}", peer_id);
            }
            WireMessageType::NetworkInfo => {
                // TODO: Implement network info exchange
                debug!("Received network info from {}", peer_id);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_basic_network_creation() {
        let config = NetworkConfig::default();
        let (network, _receiver) = ProductionP2PNetwork::new(0, "127.0.0.1:8000".parse().unwrap(), config);
        
        // Just check that we can create the network
        assert_eq!(network.node_id, 0);
        println!("Network created successfully");
    }

    #[tokio::test]
    async fn test_network_start() {
        let config = NetworkConfig::default();
        let (network, _receiver) = ProductionP2PNetwork::new(0, "127.0.0.1:8001".parse().unwrap(), config);
        
        // Test that we can start the network
        let result = network.start().await;
        assert!(result.is_ok(), "Network should start successfully");
        
        // Give it a moment to start
        sleep(Duration::from_millis(100)).await;
        
        println!("Network started successfully");
    }
}

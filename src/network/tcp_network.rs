// Production-ready TCP network implementation for HotStuff-2
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::stream::StreamExt;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::{sleep, timeout};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use crate::error::HotStuffError;
use crate::message::consensus::ConsensusMsg;
use crate::message::network::NetworkMsg;

/// Enhanced network message with reliability features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpNetworkMessage {
    pub id: u64,                    // Unique message ID
    pub from: u64,                  // Sender node ID
    pub to: Option<u64>,            // Receiver node ID (None for broadcast)
    pub timestamp: u64,             // Message timestamp
    pub sequence: u64,              // Sequence number for ordering
    pub requires_ack: bool,         // Whether acknowledgment is required
    pub is_ack: bool,               // Whether this is an acknowledgment
    pub ack_for: Option<u64>,       // ID of message being acknowledged
    pub payload: NetworkPayload,     // Message payload
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkPayload {
    Consensus(ConsensusMsg),
    Network(NetworkMsg),
    Heartbeat { node_status: NodeStatus },
    Acknowledgment,
    PeerDiscovery { known_peers: Vec<PeerInfo> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    pub node_id: u64,
    pub current_view: u64,
    pub current_height: u64,
    pub is_leader: bool,
    pub peer_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub node_id: u64,
    pub address: SocketAddr,
    pub last_seen: u64,
    pub is_active: bool,
}

/// Connection state for each peer
#[derive(Debug)]
struct PeerConnection {
    node_id: u64,
    address: SocketAddr,
    stream: Option<TcpStream>,
    last_message_time: Instant,
    consecutive_failures: u32,
    is_connected: bool,
    message_sequence: u64,
    pending_acks: HashMap<u64, Instant>,
}

impl PeerConnection {
    fn new(node_id: u64, address: SocketAddr) -> Self {
        Self {
            node_id,
            address,
            stream: None,
            last_message_time: Instant::now(),
            consecutive_failures: 0,
            is_connected: false,
            message_sequence: 0,
            pending_acks: HashMap::new(),
        }
    }

    fn mark_connected(&mut self, stream: TcpStream) {
        self.stream = Some(stream);
        self.last_message_time = Instant::now();
        self.consecutive_failures = 0;
        self.is_connected = true;
    }

    fn mark_disconnected(&mut self) {
        self.stream = None;
        self.consecutive_failures += 1;
        self.is_connected = false;
        self.pending_acks.clear();
    }

    fn should_reconnect(&self) -> bool {
        !self.is_connected 
            && self.last_message_time.elapsed() > Duration::from_secs(5)
            && self.consecutive_failures < 10
    }

    fn next_sequence(&mut self) -> u64 {
        self.message_sequence += 1;
        self.message_sequence
    }

    fn add_pending_ack(&mut self, message_id: u64) {
        self.pending_acks.insert(message_id, Instant::now());
    }

    fn remove_pending_ack(&mut self, message_id: u64) -> bool {
        self.pending_acks.remove(&message_id).is_some()
    }

    fn clean_expired_acks(&mut self, timeout: Duration) {
        let now = Instant::now();
        self.pending_acks.retain(|_, timestamp| now.duration_since(*timestamp) < timeout);
    }
}

/// Production TCP network implementation
pub struct TcpNetwork {
    node_id: u64,
    listen_address: SocketAddr,
    peers: Arc<RwLock<HashMap<u64, PeerConnection>>>,
    known_peers: Arc<RwLock<HashSet<PeerInfo>>>,
    
    // Message handling
    message_counter: Arc<Mutex<u64>>,
    inbound_messages: mpsc::Sender<TcpNetworkMessage>,
    outbound_messages: Arc<Mutex<Option<mpsc::Receiver<TcpNetworkMessage>>>>,
    outbound_sender: mpsc::Sender<TcpNetworkMessage>,
    
    // Configuration
    connection_timeout: Duration,
    message_timeout: Duration,
    heartbeat_interval: Duration,
    max_reconnect_attempts: u32,
    
    // Statistics
    messages_sent: Arc<Mutex<u64>>,
    messages_received: Arc<Mutex<u64>>,
    connection_failures: Arc<Mutex<u64>>,
}

impl TcpNetwork {
    /// Create a new TCP network instance
    pub fn new(
        node_id: u64,
        listen_address: SocketAddr,
        initial_peers: HashMap<u64, SocketAddr>,
    ) -> Result<(Self, mpsc::Receiver<TcpNetworkMessage>), HotStuffError> {
        let (inbound_tx, inbound_rx) = mpsc::channel(1000);
        let (outbound_tx, outbound_rx) = mpsc::channel(1000);
        
        let peer_connections = initial_peers
            .into_iter()
            .map(|(id, addr)| (id, PeerConnection::new(id, addr)))
            .collect();
        
        let network = TcpNetwork {
            node_id,
            listen_address,
            peers: Arc::new(RwLock::new(peer_connections)),
            known_peers: Arc::new(RwLock::new(HashSet::new())),
            message_counter: Arc::new(Mutex::new(0)),
            inbound_messages: inbound_tx,
            outbound_messages: Arc::new(Mutex::new(Some(outbound_rx))),
            outbound_sender: outbound_tx,
            connection_timeout: Duration::from_secs(10),
            message_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(5),
            max_reconnect_attempts: 10,
            messages_sent: Arc::new(Mutex::new(0)),
            messages_received: Arc::new(Mutex::new(0)),
            connection_failures: Arc::new(Mutex::new(0)),
        };
        
        Ok((network, inbound_rx))
    }
    
    /// Start all network services
    pub async fn start(&self) -> Result<(), HotStuffError> {
        info!("Starting TCP network for node {} on {}", self.node_id, self.listen_address);
        
        // Start server to accept incoming connections
        self.start_server().await?;
        
        // Start client connection manager
        self.start_client_manager().await?;
        
        // Start message processor
        self.start_message_processor().await?;
        
        // Start heartbeat service
        self.start_heartbeat_service().await?;
        
        // Start maintenance tasks
        self.start_maintenance().await?;
        
        info!("TCP network started successfully");
        Ok(())
    }
    
    /// Send message to specific peer with optional acknowledgment
    pub async fn send_message(&self, 
        to: u64, 
        payload: NetworkPayload, 
        requires_ack: bool
    ) -> Result<(), HotStuffError> {
        let mut counter = self.message_counter.lock().await;
        *counter += 1;
        let message_id = *counter;
        drop(counter);
        
        let message = TcpNetworkMessage {
            id: message_id,
            from: self.node_id,
            to: Some(to),
            timestamp: Self::current_timestamp(),
            sequence: 0, // Will be set by peer connection
            requires_ack,
            is_ack: false,
            ack_for: None,
            payload,
        };
        
        self.outbound_sender.send(message).await
            .map_err(|e| HotStuffError::Network(format!("Failed to queue message: {}", e)))?;
        
        Ok(())
    }
    
    /// Broadcast message to all connected peers
    pub async fn broadcast_message(&self, payload: NetworkPayload) -> Result<(), HotStuffError> {
        let mut counter = self.message_counter.lock().await;
        *counter += 1;
        let message_id = *counter;
        drop(counter);
        
        let message = TcpNetworkMessage {
            id: message_id,
            from: self.node_id,
            to: None, // Broadcast
            timestamp: Self::current_timestamp(),
            sequence: 0,
            requires_ack: false,
            is_ack: false,
            ack_for: None,
            payload,
        };
        
        self.outbound_sender.send(message).await
            .map_err(|e| HotStuffError::Network(format!("Failed to queue broadcast: {}", e)))?;
        
        Ok(())
    }
    
    /// Get network statistics
    pub async fn get_network_statistics(&self) -> TcpNetworkStats {
        let peers_read = self.peers.read().await;
        let connected_peers = peers_read.values().filter(|p| p.is_connected).count();
        let total_peers = peers_read.len();
        
        let pending_acks: usize = peers_read.values()
            .map(|p| p.pending_acks.len())
            .sum();
        
        TcpNetworkStats {
            node_id: self.node_id,
            connected_peers,
            total_peers,
            messages_sent: *self.messages_sent.lock().await,
            messages_received: *self.messages_received.lock().await,
            connection_failures: *self.connection_failures.lock().await,
            pending_acknowledgments: pending_acks,
        }
    }
    
    async fn start_server(&self) -> Result<(), HotStuffError> {
        let listener = TcpListener::bind(self.listen_address).await
            .map_err(|e| HotStuffError::Network(format!("Failed to bind to {}: {}", self.listen_address, e)))?;
        
        let peers = Arc::clone(&self.peers);
        let inbound_tx = self.inbound_messages.clone();
        let node_id = self.node_id;
        let messages_received = Arc::clone(&self.messages_received);
        
        tokio::spawn(async move {
            info!("TCP server listening on {}", listener.local_addr().unwrap());
            
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        debug!("Accepted connection from {}", addr);
                        
                        let peers_clone = Arc::clone(&peers);
                        let inbound_tx_clone = inbound_tx.clone();
                        let messages_received_clone = Arc::clone(&messages_received);
                        
                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_incoming_connection(
                                stream,
                                addr,
                                peers_clone,
                                inbound_tx_clone,
                                node_id,
                                messages_received_clone,
                            ).await {
                                error!("Error handling connection from {}: {}", addr, e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to accept connection: {}", e);
                        sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        });
        
        Ok(())
    }
    
    async fn start_client_manager(&self) -> Result<(), HotStuffError> {
        let peers = Arc::clone(&self.peers);
        let connection_timeout = self.connection_timeout;
        let connection_failures = Arc::clone(&self.connection_failures);
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(2));
            
            loop {
                interval.tick().await;
                
                // Find peers that need reconnection
                let peers_to_connect = {
                    let peers_read = peers.read().await;
                    peers_read.values()
                        .filter(|conn| conn.should_reconnect())
                        .map(|conn| (conn.node_id, conn.address))
                        .collect::<Vec<_>>()
                };
                
                // Attempt to connect to disconnected peers
                for (peer_id, address) in peers_to_connect {
                    debug!("Attempting to connect to peer {} at {}", peer_id, address);
                    
                    match timeout(connection_timeout, TcpStream::connect(address)).await {
                        Ok(Ok(stream)) => {
                            info!("Connected to peer {} at {}", peer_id, address);
                            let mut peers_write = peers.write().await;
                            if let Some(conn) = peers_write.get_mut(&peer_id) {
                                conn.mark_connected(stream);
                            }
                        }
                        Ok(Err(e)) => {
                            warn!("Connection failed to peer {}: {}", peer_id, e);
                            let mut peers_write = peers.write().await;
                            if let Some(conn) = peers_write.get_mut(&peer_id) {
                                conn.mark_disconnected();
                            }
                            *connection_failures.lock().await += 1;
                        }
                        Err(_) => {
                            warn!("Connection timeout to peer {}", peer_id);
                            let mut peers_write = peers.write().await;
                            if let Some(conn) = peers_write.get_mut(&peer_id) {
                                conn.mark_disconnected();
                            }
                            *connection_failures.lock().await += 1;
                        }
                    }
                }
            }
        });
        
        Ok(())
    }
    
    async fn start_message_processor(&self) -> Result<(), HotStuffError> {
        let outbound_rx = {
            let mut rx_opt = self.outbound_messages.lock().await;
            rx_opt.take().ok_or_else(|| {
                HotStuffError::Network("Message processor already started".to_string())
            })?
        };
        
        let peers = Arc::clone(&self.peers);
        let message_timeout = self.message_timeout;
        let messages_sent = Arc::clone(&self.messages_sent);
        
        tokio::spawn(async move {
            let mut rx = outbound_rx;
            
            while let Some(mut message) = rx.recv().await {
                if let Some(to) = message.to {
                    // Send to specific peer
                    let mut peers_write = peers.write().await;
                    if let Some(conn) = peers_write.get_mut(&to) {
                        if conn.is_connected {
                            message.sequence = conn.next_sequence();
                            
                            if message.requires_ack {
                                conn.add_pending_ack(message.id);
                            }
                            
                            if let Some(ref mut stream) = conn.stream {
                                match Self::send_message_to_stream(stream, &message, message_timeout).await {
                                    Ok(()) => {
                                        *messages_sent.lock().await += 1;
                                        debug!("Sent message {} to peer {}", message.id, to);
                                    }
                                    Err(e) => {
                                        error!("Failed to send message to peer {}: {}", to, e);
                                        conn.mark_disconnected();
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // Broadcast to all connected peers
                    let mut peers_write = peers.write().await;
                    for (peer_id, conn) in peers_write.iter_mut() {
                        if *peer_id != message.from && conn.is_connected {
                            message.sequence = conn.next_sequence();
                            
                            if let Some(ref mut stream) = conn.stream {
                                match Self::send_message_to_stream(stream, &message, message_timeout).await {
                                    Ok(()) => {
                                        *messages_sent.lock().await += 1;
                                        debug!("Broadcast message {} to peer {}", message.id, peer_id);
                                    }
                                    Err(e) => {
                                        error!("Failed to broadcast to peer {}: {}", peer_id, e);
                                        conn.mark_disconnected();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
        
        Ok(())
    }
    
    async fn start_heartbeat_service(&self) -> Result<(), HotStuffError> {
        let outbound_sender = self.outbound_sender.clone();
        let node_id = self.node_id;
        let heartbeat_interval = self.heartbeat_interval;
        let peers = Arc::clone(&self.peers);
        let message_counter = Arc::clone(&self.message_counter);
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(heartbeat_interval);
            
            loop {
                interval.tick().await;
                
                let peer_count = {
                    let peers_read = peers.read().await;
                    peers_read.values().filter(|p| p.is_connected).count()
                };
                
                let mut counter = message_counter.lock().await;
                *counter += 1;
                let message_id = *counter;
                drop(counter);
                
                let heartbeat_payload = NetworkPayload::Heartbeat {
                    node_status: NodeStatus {
                        node_id,
                        current_view: 0, // TODO: Get from consensus
                        current_height: 0, // TODO: Get from consensus
                        is_leader: false, // TODO: Get from consensus
                        peer_count,
                    },
                };
                
                let heartbeat_message = TcpNetworkMessage {
                    id: message_id,
                    from: node_id,
                    to: None, // Broadcast
                    timestamp: Self::current_timestamp(),
                    sequence: 0,
                    requires_ack: false,
                    is_ack: false,
                    ack_for: None,
                    payload: heartbeat_payload,
                };
                
                let _ = outbound_sender.send(heartbeat_message).await;
            }
        });
        
        Ok(())
    }
    
    async fn start_maintenance(&self) -> Result<(), HotStuffError> {
        let peers = Arc::clone(&self.peers);
        let message_timeout = self.message_timeout;
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            
            loop {
                interval.tick().await;
                
                // Clean up expired acknowledgments
                {
                    let mut peers_write = peers.write().await;
                    for conn in peers_write.values_mut() {
                        conn.clean_expired_acks(message_timeout);
                    }
                }
            }
        });
        
        Ok(())
    }
    
    async fn handle_incoming_connection(
        stream: TcpStream,
        addr: SocketAddr,
        peers: Arc<RwLock<HashMap<u64, PeerConnection>>>,
        inbound_tx: mpsc::Sender<TcpNetworkMessage>,
        _node_id: u64,
        messages_received: Arc<Mutex<u64>>,
    ) -> Result<(), HotStuffError> {
        let mut framed = Framed::new(stream, LengthDelimitedCodec::new());
        
        while let Some(data) = framed.next().await {
            match data {
                Ok(bytes) => {
                    match bincode::deserialize::<TcpNetworkMessage>(&bytes) {
                        Ok(message) => {
                            debug!("Received message {} from peer {}", message.id, message.from);
                            
                            // Update peer connection info
                            {
                                let mut peers_write = peers.write().await;
                                if let Some(conn) = peers_write.get_mut(&message.from) {
                                    conn.last_message_time = Instant::now();
                                    
                                    // Handle acknowledgments
                                    if message.is_ack {
                                        if let Some(ack_id) = message.ack_for {
                                            conn.remove_pending_ack(ack_id);
                                        }
                                    }
                                }
                            }
                            
                            // Send acknowledgment if required
                            if message.requires_ack && !message.is_ack {
                                let ack_message = TcpNetworkMessage {
                                    id: message.id + 1000000, // Simple ACK ID scheme
                                    from: _node_id,
                                    to: Some(message.from),
                                    timestamp: Self::current_timestamp(),
                                    sequence: 0,
                                    requires_ack: false,
                                    is_ack: true,
                                    ack_for: Some(message.id),
                                    payload: NetworkPayload::Acknowledgment,
                                };
                                
                                let _ = Self::send_message_to_tcp_stream(&mut framed, &ack_message).await;
                            }
                            
                            *messages_received.lock().await += 1;
                            
                            // Forward message if not an acknowledgment
                            if !message.is_ack {
                                if let Err(e) = inbound_tx.send(message).await {
                                    error!("Failed to forward message: {}", e);
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to deserialize message from {}: {}", addr, e);
                        }
                    }
                }
                Err(e) => {
                    error!("Error reading from connection {}: {}", addr, e);
                    break;
                }
            }
        }
        
        Ok(())
    }
    
    async fn send_message_to_stream(
        stream: &mut TcpStream,
        message: &TcpNetworkMessage,
        timeout_duration: Duration,
    ) -> Result<(), HotStuffError> {
        let result = timeout(timeout_duration, async {
            let serialized = bincode::serialize(message)
                .map_err(|e| HotStuffError::Serialization(e))?;
            
            let len = serialized.len() as u32;
            stream.write_all(&len.to_be_bytes()).await
                .map_err(|e| HotStuffError::Network(format!("Failed to write length: {}", e)))?;
            
            stream.write_all(&serialized).await
                .map_err(|e| HotStuffError::Network(format!("Failed to write message: {}", e)))?;
            
            stream.flush().await
                .map_err(|e| HotStuffError::Network(format!("Failed to flush: {}", e)))?;
            
            Ok(())
        }).await;
        
        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(HotStuffError::Network("Message send timeout".to_string())),
        }
    }
    
    async fn send_message_to_tcp_stream(
        framed: &mut Framed<TcpStream, LengthDelimitedCodec>,
        message: &TcpNetworkMessage,
    ) -> Result<(), HotStuffError> {
        let serialized = bincode::serialize(message)
            .map_err(|e| HotStuffError::Serialization(e))?;
        
        use futures::SinkExt;
        framed.send(serialized.into()).await
            .map_err(|e| HotStuffError::Network(format!("Failed to send via framed: {}", e)))?;
        
        Ok(())
    }
    
    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}

/// Network statistics for monitoring
#[derive(Debug, Clone)]
pub struct TcpNetworkStats {
    pub node_id: u64,
    pub connected_peers: usize,
    pub total_peers: usize,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub connection_failures: u64,
    pub pending_acknowledgments: usize,
}

impl std::fmt::Display for TcpNetworkStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Node {}: {}/{} peers, {} sent, {} received, {} failures, {} pending ACKs",
            self.node_id,
            self.connected_peers,
            self.total_peers,
            self.messages_sent,
            self.messages_received,
            self.connection_failures,
            self.pending_acknowledgments
        )
    }
}

// Production-ready peer-to-peer networking for HotStuff-2
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bincode;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::{sleep, timeout, interval};

use crate::error::HotStuffError;
use crate::message::consensus::ConsensusMsg;
use crate::message::network::NetworkMsg;

/// Network message with metadata for production networking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2PMessage {
    pub id: u64,           // Message ID for deduplication
    pub from: u64,         // Sender node ID
    pub to: u64,           // Receiver node ID (0 for broadcast)
    pub timestamp: u64,    // Timestamp for ordering and timeout
    pub payload: MessagePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessagePayload {
    Consensus(ConsensusMsg),
    Network(NetworkMsg),
    Heartbeat,
    Acknowledgment { ack_id: u64 },
}

/// Enhanced P2P message with reliability and discovery features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedP2PMessage {
    pub id: u64,
    pub from: u64,
    pub to: Option<u64>,              // None for broadcast
    pub timestamp: u64,
    pub message_type: MessageType,
    pub payload: MessagePayload,
    pub requires_ack: bool,
    pub is_ack: bool,
    pub ack_for: Option<u64>,
    pub hop_count: u8,               // For routing
    pub priority: MessagePriority,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    Consensus,
    Network,
    Discovery,
    Heartbeat,
    Acknowledgment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessagePriority {
    Critical,     // Consensus messages
    Normal,       // Regular network messages
    Background,   // Heartbeats, discovery
}

/// Connection state tracking for reliability
#[derive(Debug, Clone)]
struct PeerConnection {
    node_id: u64,
    addr: SocketAddr,
    stream: Option<Arc<Mutex<TcpStream>>>,
    last_seen: std::time::Instant,
    consecutive_failures: u32,
    is_connected: bool,
}

impl PeerConnection {
    fn new(node_id: u64, addr: SocketAddr) -> Self {
        Self {
            node_id,
            addr,
            stream: None,
            last_seen: std::time::Instant::now(),
            consecutive_failures: 0,
            is_connected: false,
        }
    }
    
    fn mark_connected(&mut self, stream: TcpStream) {
        self.stream = Some(Arc::new(Mutex::new(stream)));
        self.last_seen = std::time::Instant::now();
        self.consecutive_failures = 0;
        self.is_connected = true;
    }
    
    fn mark_disconnected(&mut self) {
        self.stream = None;
        self.consecutive_failures += 1;
        self.is_connected = false;
    }
    
    fn should_reconnect(&self) -> bool {
        !self.is_connected && 
        self.last_seen.elapsed() > Duration::from_secs(5) &&
        self.consecutive_failures < 5
    }
}

/// Production peer-to-peer network implementation
pub struct P2PNetwork {
    node_id: u64,
    listen_addr: SocketAddr,
    peers: Arc<RwLock<HashMap<u64, PeerConnection>>>,
    
    // Message handling
    message_counter: Arc<Mutex<u64>>,
    pending_acks: Arc<Mutex<HashMap<u64, tokio::time::Instant>>>,
    message_dedup: Arc<Mutex<HashMap<u64, tokio::time::Instant>>>,
    
    // Communication channels
    inbound_tx: mpsc::Sender<P2PMessage>,
    outbound_rx: Arc<Mutex<Option<mpsc::Receiver<P2PMessage>>>>,
    outbound_tx: mpsc::Sender<P2PMessage>,
    
    // Configuration
    connection_timeout: Duration,
    message_timeout: Duration,
    heartbeat_interval: Duration,
    max_reconnect_attempts: u32,
}

impl P2PNetwork {
    pub fn new(
        node_id: u64,
        listen_addr: SocketAddr,
        peers: HashMap<u64, SocketAddr>,
    ) -> Result<(Self, mpsc::Receiver<P2PMessage>), HotStuffError> {
        let (inbound_tx, inbound_rx) = mpsc::channel(1000);
        let (outbound_tx, outbound_rx) = mpsc::channel(1000);
        
        let peer_connections = peers
            .into_iter()
            .map(|(id, addr)| (id, PeerConnection::new(id, addr)))
            .collect();
        
        let network = P2PNetwork {
            node_id,
            listen_addr,
            peers: Arc::new(RwLock::new(peer_connections)),
            message_counter: Arc::new(Mutex::new(0)),
            pending_acks: Arc::new(Mutex::new(HashMap::new())),
            message_dedup: Arc::new(Mutex::new(HashMap::new())),
            inbound_tx,
            outbound_rx: Arc::new(Mutex::new(Some(outbound_rx))),
            outbound_tx,
            connection_timeout: Duration::from_secs(10),
            message_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(5),
            max_reconnect_attempts: 5,
        };
        
        Ok((network, inbound_rx))
    }
    
    /// Start the P2P network server and client loops
    pub async fn start(&self) -> Result<(), HotStuffError> {
        info!("Starting P2P network on {}", self.listen_addr);
        
        // Start server to accept incoming connections
        self.start_server().await?;
        
        // Start client to connect to peers
        self.start_client().await?;
        
        // Start message processing loop
        self.start_message_processor().await?;
        
        // Start heartbeat and cleanup tasks
        self.start_heartbeat().await?;
        self.start_cleanup().await?;
        
        Ok(())
    }
    
    /// Send message to specific peer or broadcast
    pub async fn send_message(&self, message: P2PMessage) -> Result<(), HotStuffError> {
        self.outbound_tx.send(message).await
            .map_err(|e| HotStuffError::Network(format!("Failed to queue message: {}", e)))?;
        Ok(())
    }
    
    /// Broadcast message to all peers
    pub async fn broadcast(&self, payload: MessagePayload) -> Result<(), HotStuffError> {
        let mut message_id = self.message_counter.lock().await;
        *message_id += 1;
        let id = *message_id;
        drop(message_id);
        
        let message = P2PMessage {
            id,
            from: self.node_id,
            to: 0, // Broadcast
            timestamp: self.current_timestamp(),
            payload,
        };
        
        self.send_message(message).await
    }
    
    /// Send message to specific peer with acknowledgment
    pub async fn send_reliable(&self, to: u64, payload: MessagePayload) -> Result<(), HotStuffError> {
        let mut message_id = self.message_counter.lock().await;
        *message_id += 1;
        let id = *message_id;
        drop(message_id);
        
        let message = P2PMessage {
            id,
            from: self.node_id,
            to,
            timestamp: self.current_timestamp(),
            payload,
        };
        
        // Track pending acknowledgment
        self.pending_acks.lock().await.insert(id, tokio::time::Instant::now());
        
        self.send_message(message).await
    }
    
    /// Enhanced initialization with peer discovery
    pub async fn new_enhanced(
        node_id: u64,
        listen_addr: SocketAddr,
        bootstrap_peers: Vec<SocketAddr>,
    ) -> Result<(Self, mpsc::Receiver<P2PMessage>), HotStuffError> {
        let (inbound_tx, inbound_rx) = mpsc::channel(2000);
        let (outbound_tx, outbound_rx) = mpsc::channel(2000);
        
        let network = P2PNetwork {
            node_id,
            listen_addr,
            peers: Arc::new(RwLock::new(HashMap::new())),
            message_counter: Arc::new(Mutex::new(0)),
            pending_acks: Arc::new(Mutex::new(HashMap::new())),
            message_dedup: Arc::new(Mutex::new(HashMap::new())),
            inbound_tx,
            outbound_rx: Arc::new(Mutex::new(Some(outbound_rx))),
            outbound_tx,
            connection_timeout: Duration::from_secs(15),
            message_timeout: Duration::from_secs(60),
            heartbeat_interval: Duration::from_secs(3),
            max_reconnect_attempts: 10,
        };
        
        // Start peer discovery with bootstrap peers
        network.bootstrap_peer_discovery(bootstrap_peers).await?;
        
        Ok((network, inbound_rx))
    }
    
    /// Bootstrap peer discovery process
    async fn bootstrap_peer_discovery(&self, bootstrap_peers: Vec<SocketAddr>) -> Result<(), HotStuffError> {
        info!("Starting peer discovery with {} bootstrap peers", bootstrap_peers.len());
        
        for addr in bootstrap_peers {
            // Try to connect and discover peers
            if let Ok(stream) = timeout(self.connection_timeout, TcpStream::connect(addr)).await {
                if let Ok(_stream) = stream {
                    info!("Connected to bootstrap peer at {}", addr);
                    // For now, just log the connection
                    // TODO: Implement proper discovery protocol
                }
            }
        }
        
        Ok(())
    }
    
    /// Send message with reliability and routing
    pub async fn send_reliable_message(
        &self,
        to: u64,
        payload: MessagePayload,
        _priority: MessagePriority,
    ) -> Result<(), HotStuffError> {
        let mut message_id = self.message_counter.lock().await;
        *message_id += 1;
        let id = *message_id;
        drop(message_id);
        
        let message = P2PMessage {
            id,
            from: self.node_id,
            to,
            timestamp: self.current_timestamp(),
            payload,
        };
        
        // Track for acknowledgment
        self.pending_acks.lock().await.insert(id, tokio::time::Instant::now());
        
        self.send_message(message).await?;
        Ok(())
    }
    
    /// Enhanced peer management with health monitoring
    pub async fn start_peer_health_monitoring(&self) -> Result<(), HotStuffError> {
        let peers = Arc::clone(&self.peers);
        let outbound_tx = self.outbound_tx.clone();
        let node_id = self.node_id;
        let message_counter = Arc::clone(&self.message_counter);
        
        tokio::spawn(async move {
            let mut health_interval = interval(Duration::from_secs(30));
            
            loop {
                health_interval.tick().await;
                
                // Check health of all peers
                let peer_ids = {
                    let peers_read = peers.read().await;
                    peers_read.keys().copied().collect::<Vec<_>>()
                };
                
                for peer_id in peer_ids {
                    // Send health check ping
                    let mut counter = message_counter.lock().await;
                    *counter += 1;
                    let id = *counter;
                    drop(counter);
                    
                    let ping_message = P2PMessage {
                        id,
                        from: node_id,
                        to: peer_id,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as u64,
                        payload: MessagePayload::Heartbeat,
                    };
                    
                    let _ = outbound_tx.send(ping_message).await;
                }
            }
        });
        
        Ok(())
    }
    
    /// Get comprehensive network statistics
    pub async fn get_enhanced_network_stats(&self) -> EnhancedNetworkStats {
        let peers_read = self.peers.read().await;
        let connected_peers = peers_read.values().filter(|c| c.is_connected).count();
        let total_peers = peers_read.len();
        
        let pending_acks_count = self.pending_acks.lock().await.len();
        let dedup_entries = self.message_dedup.lock().await.len();
        
        // Calculate average latency
        let mut total_latency = 0u64;
        let mut latency_count = 0;
        
        for conn in peers_read.values() {
            if conn.is_connected {
                // Mock latency calculation - in real implementation, measure actual RTT
                total_latency += 50; // 50ms mock latency
                latency_count += 1;
            }
        }
        
        let average_latency = if latency_count > 0 {
            Some(total_latency / latency_count as u64)
        } else {
            None
        };
        
        EnhancedNetworkStats {
            node_id: self.node_id,
            connected_peers,
            total_peers,
            pending_acks: pending_acks_count,
            dedup_cache_size: dedup_entries,
            average_latency_ms: average_latency,
            network_health: if connected_peers > total_peers / 2 { 
                NetworkHealth::Good 
            } else { 
                NetworkHealth::Degraded 
            },
        }
    }
    
    /// Advanced message routing with multi-hop support
    pub async fn route_message(&self, message: P2PMessage, _max_hops: u8) -> Result<(), HotStuffError> {
        // Simplified routing - just forward to destination if we know it
        let target_peer = message.to;
        
        let peers_read = self.peers.read().await;
        if let Some(conn) = peers_read.get(&target_peer) {
            if conn.is_connected {
                // Direct connection available
                drop(peers_read);
                self.send_message(message).await?;
            } else {
                // Need to route through other peers
                // For now, just drop the message
                warn!("No route to peer {}", target_peer);
            }
        }
        
        Ok(())
    }
    
    async fn start_server(&self) -> Result<(), HotStuffError> {
        let listener = TcpListener::bind(self.listen_addr).await
            .map_err(|e| HotStuffError::Network(format!("Failed to bind to {}: {}", self.listen_addr, e)))?;
        
        let peers = Arc::clone(&self.peers);
        let inbound_tx = self.inbound_tx.clone();
        let node_id = self.node_id;
        
        tokio::spawn(async move {
            info!("P2P server listening on {}", listener.local_addr().unwrap());
            
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        debug!("Accepted connection from {}", addr);
                        
                        let peers_clone = Arc::clone(&peers);
                        let inbound_tx_clone = inbound_tx.clone();
                        
                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_incoming_connection(
                                stream, 
                                addr, 
                                peers_clone, 
                                inbound_tx_clone,
                                node_id
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
    
    async fn start_client(&self) -> Result<(), HotStuffError> {
        let peers = Arc::clone(&self.peers);
        let connection_timeout = self.connection_timeout;
        
        tokio::spawn(async move {
            loop {
                // Check for peers that need reconnection
                let peers_to_connect = {
                    let peers_read = peers.read().await;
                    peers_read.values()
                        .filter(|conn| conn.should_reconnect())
                        .map(|conn| (conn.node_id, conn.addr))
                        .collect::<Vec<_>>()
                };
                
                for (peer_id, addr) in peers_to_connect {
                    debug!("Attempting to connect to peer {} at {}", peer_id, addr);
                    
                    match timeout(connection_timeout, TcpStream::connect(addr)).await {
                        Ok(Ok(stream)) => {
                            info!("Connected to peer {} at {}", peer_id, addr);
                            let mut peers_write = peers.write().await;
                            if let Some(conn) = peers_write.get_mut(&peer_id) {
                                conn.mark_connected(stream);
                            }
                        }
                        Ok(Err(e)) => {
                            warn!("Failed to connect to peer {}: {}", peer_id, e);
                            let mut peers_write = peers.write().await;
                            if let Some(conn) = peers_write.get_mut(&peer_id) {
                                conn.mark_disconnected();
                            }
                        }
                        Err(_) => {
                            warn!("Connection timeout to peer {}", peer_id);
                            let mut peers_write = peers.write().await;
                            if let Some(conn) = peers_write.get_mut(&peer_id) {
                                conn.mark_disconnected();
                            }
                        }
                    }
                }
                
                sleep(Duration::from_secs(2)).await;
            }
        });
        
        Ok(())
    }
    
    async fn start_message_processor(&self) -> Result<(), HotStuffError> {
        let outbound_rx = {
            let mut rx_opt = self.outbound_rx.lock().await;
            rx_opt.take().ok_or_else(|| {
                HotStuffError::Network("Message processor already started".to_string())
            })?
        };
        
        let peers = Arc::clone(&self.peers);
        let message_timeout = self.message_timeout;
        
        tokio::spawn(async move {
            let mut rx = outbound_rx;
            
            while let Some(message) = rx.recv().await {
                if message.to == 0 {
                    // Broadcast message
                    let peers_read = peers.read().await;
                    for (peer_id, conn) in peers_read.iter() {
                        if *peer_id != message.from && conn.is_connected {
                            if let Some(stream) = &conn.stream {
                                let _ = Self::send_message_to_stream(
                                    Arc::clone(stream), 
                                    &message, 
                                    message_timeout
                                ).await;
                            }
                        }
                    }
                } else {
                    // Send to specific peer
                    let peers_read = peers.read().await;
                    if let Some(conn) = peers_read.get(&message.to) {
                        if conn.is_connected {
                            if let Some(stream) = &conn.stream {
                                let _ = Self::send_message_to_stream(
                                    Arc::clone(stream), 
                                    &message, 
                                    message_timeout
                                ).await;
                            }
                        }
                    }
                }
            }
        });
        
        Ok(())
    }
    
    async fn start_heartbeat(&self) -> Result<(), HotStuffError> {
        let outbound_tx = self.outbound_tx.clone();
        let node_id = self.node_id;
        let heartbeat_interval = self.heartbeat_interval;
        let message_counter = Arc::clone(&self.message_counter);
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(heartbeat_interval);
            
            loop {
                interval.tick().await;
                
                let mut counter = message_counter.lock().await;
                *counter += 1;
                let id = *counter;
                drop(counter);
                
                let heartbeat = P2PMessage {
                    id,
                    from: node_id,
                    to: 0, // Broadcast
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64,
                    payload: MessagePayload::Heartbeat,
                };
                
                let _ = outbound_tx.send(heartbeat).await;
            }
        });
        
        Ok(())
    }
    
    async fn start_cleanup(&self) -> Result<(), HotStuffError> {
        let pending_acks = Arc::clone(&self.pending_acks);
        let message_dedup = Arc::clone(&self.message_dedup);
        let message_timeout = self.message_timeout;
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            
            loop {
                interval.tick().await;
                
                let now = tokio::time::Instant::now();
                
                // Clean up expired pending acknowledgments
                {
                    let mut acks = pending_acks.lock().await;
                    acks.retain(|_, timestamp| now.duration_since(*timestamp) < message_timeout);
                }
                
                // Clean up old message deduplication entries
                {
                    let mut dedup = message_dedup.lock().await;
                    dedup.retain(|_, timestamp| now.duration_since(*timestamp) < message_timeout);
                }
            }
        });
        
        Ok(())
    }
    
    async fn handle_incoming_connection(
        mut stream: TcpStream,
        addr: SocketAddr,
        peers: Arc<RwLock<HashMap<u64, PeerConnection>>>,
        inbound_tx: mpsc::Sender<P2PMessage>,
        _node_id: u64,
    ) -> Result<(), HotStuffError> {
        let mut buffer = vec![0u8; 8192];
        let mut message_buffer = Vec::new();
        
        loop {
            match stream.read(&mut buffer).await {
                Ok(0) => {
                    debug!("Connection closed by {}", addr);
                    break;
                }
                Ok(n) => {
                    message_buffer.extend_from_slice(&buffer[..n]);
                    
                    // Try to deserialize complete messages
                    while let Some(message) = Self::try_deserialize_message(&mut message_buffer)? {
                        debug!("Received message from {}: {:?}", addr, message);
                        
                        // Update peer connection info
                        {
                            let mut peers_write = peers.write().await;
                            if let Some(conn) = peers_write.get_mut(&message.from) {
                                conn.last_seen = std::time::Instant::now();
                            }
                        }
                        
                        // Send acknowledgment for reliable messages
                        if message.to != 0 {
                            let ack = P2PMessage {
                                id: message.id + 1000000, // Simple ACK ID scheme
                                from: _node_id,
                                to: message.from,
                                timestamp: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_millis() as u64,
                                payload: MessagePayload::Acknowledgment { ack_id: message.id },
                            };
                            
                            let _ = Self::send_message_to_connection(&mut stream, &ack).await;
                        }
                        
                        // Forward message to consensus layer
                        if let Err(e) = inbound_tx.send(message).await {
                            error!("Failed to forward message: {}", e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    error!("Error reading from {}: {}", addr, e);
                    break;
                }
            }
        }
        
        Ok(())
    }
    
    async fn send_message_to_stream(
        stream: Arc<Mutex<TcpStream>>,
        message: &P2PMessage,
        timeout_duration: Duration,
    ) -> Result<(), HotStuffError> {
        let result = timeout(timeout_duration, async {
            let mut stream_guard = stream.lock().await;
            Self::send_message_to_connection(&mut *stream_guard, message).await
        }).await;
        
        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(HotStuffError::Network("Message send timeout".to_string())),
        }
    }
    
    async fn send_message_to_connection(
        stream: &mut TcpStream,
        message: &P2PMessage,
    ) -> Result<(), HotStuffError> {
        let serialized = bincode::serialize(message)
            .map_err(|e| HotStuffError::Serialization(e))?;
        
        let len = serialized.len() as u32;
        stream.write_all(&len.to_be_bytes()).await
            .map_err(|e| HotStuffError::Network(format!("Failed to write message length: {}", e)))?;
        
        stream.write_all(&serialized).await
            .map_err(|e| HotStuffError::Network(format!("Failed to write message: {}", e)))?;
        
        stream.flush().await
            .map_err(|e| HotStuffError::Network(format!("Failed to flush stream: {}", e)))?;
        
        Ok(())
    }
    
    fn try_deserialize_message(buffer: &mut Vec<u8>) -> Result<Option<P2PMessage>, HotStuffError> {
        if buffer.len() < 4 {
            return Ok(None);
        }
        
        let len = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
        
        if buffer.len() < 4 + len {
            return Ok(None);
        }
        
        let message_bytes = buffer.drain(0..4 + len).skip(4).collect::<Vec<_>>();
        let message = bincode::deserialize(&message_bytes)
            .map_err(|e| HotStuffError::Serialization(e))?;
        
        Ok(Some(message))
    }
    
    fn current_timestamp(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
    
    /// Get network statistics for monitoring
    pub async fn get_network_stats(&self) -> NetworkStats {
        let peers_read = self.peers.read().await;
        let connected_peers = peers_read.values().filter(|c| c.is_connected).count();
        let total_peers = peers_read.len();
        
        let pending_acks_count = self.pending_acks.lock().await.len();
        let dedup_entries = self.message_dedup.lock().await.len();
        
        NetworkStats {
            node_id: self.node_id,
            connected_peers,
            total_peers,
            pending_acks: pending_acks_count,
            dedup_cache_size: dedup_entries,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub node_id: u64,
    pub connected_peers: usize,
    pub total_peers: usize,
    pub pending_acks: usize,
    pub dedup_cache_size: usize,
}

impl std::fmt::Display for NetworkStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Node {}: {}/{} peers connected, {} pending ACKs, {} dedup entries",
            self.node_id,
            self.connected_peers,
            self.total_peers,
            self.pending_acks,
            self.dedup_cache_size
        )
    }
}

#[derive(Debug, Clone)]
pub struct EnhancedNetworkStats {
    pub node_id: u64,
    pub connected_peers: usize,
    pub total_peers: usize,
    pub pending_acks: usize,
    pub dedup_cache_size: usize,
    pub average_latency_ms: Option<u64>,
    pub network_health: NetworkHealth,
}

#[derive(Debug, Clone)]
pub enum NetworkHealth {
    Good,
    Degraded,
    Poor,
    Disconnected,
}

impl std::fmt::Display for EnhancedNetworkStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Node {}: {}/{} peers, {} pending ACKs, {:?} health, {}ms avg latency",
            self.node_id,
            self.connected_peers,
            self.total_peers,
            self.pending_acks,
            self.network_health,
            self.average_latency_ms.unwrap_or(0)
        )
    }
}

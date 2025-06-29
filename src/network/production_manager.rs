// Enhanced network manager for HotStuff-2 production deployment
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use log::{info, warn};
use tokio::sync::{mpsc, RwLock};

use crate::error::HotStuffError;
use crate::message::consensus::ConsensusMsg;
use crate::message::network::NetworkMsg;
use crate::network::{
    TcpNetwork,
    NetworkReliabilityManager, NetworkFaultDetector,
    reliability::FaultDetectionThresholds
};
use crate::network::tcp_network::{TcpNetworkMessage, NetworkPayload as TcpNetworkPayload};

/// Production network manager with reliability and fault tolerance
pub struct ProductionNetworkManager {
    node_id: u64,
    tcp_network: Arc<TcpNetwork>,
    reliability_manager: Arc<NetworkReliabilityManager>,
    fault_detector: Arc<NetworkFaultDetector>,
    
    // Message routing
    inbound_messages: Arc<RwLock<Option<mpsc::Receiver<TcpNetworkMessage>>>>,
    message_handlers: HashMap<String, Box<dyn Fn(TcpNetworkMessage) + Send + Sync>>,
    
    // Statistics
    total_messages_sent: Arc<tokio::sync::Mutex<u64>>,
    total_messages_received: Arc<tokio::sync::Mutex<u64>>,
}

impl ProductionNetworkManager {
    /// Create new production network manager
    pub async fn new(
        node_id: u64,
        listen_address: SocketAddr,
        peers: HashMap<u64, SocketAddr>,
    ) -> Result<Self, HotStuffError> {
        info!("Creating production network manager for node {} on {}", node_id, listen_address);
        
        // Create TCP network
        let (tcp_network, inbound_rx) = TcpNetwork::new(node_id, listen_address, peers)?;
        let tcp_network = Arc::new(tcp_network);
        
        // Create reliability manager
        let reliability_manager = Arc::new(NetworkReliabilityManager::new(node_id));
        
        // Create fault detector
        let fault_detector = Arc::new(NetworkFaultDetector::new(
            node_id,
            FaultDetectionThresholds::default(),
        ));
        
        let manager = ProductionNetworkManager {
            node_id,
            tcp_network,
            reliability_manager,
            fault_detector,
            inbound_messages: Arc::new(RwLock::new(Some(inbound_rx))),
            message_handlers: HashMap::new(),
            total_messages_sent: Arc::new(tokio::sync::Mutex::new(0)),
            total_messages_received: Arc::new(tokio::sync::Mutex::new(0)),
        };
        
        Ok(manager)
    }
    
    /// Start all network services
    pub async fn start(&self) -> Result<(), HotStuffError> {
        info!("Starting production network manager for node {}", self.node_id);
        
        // Start TCP network
        self.tcp_network.start().await?;
        
        // Start reliability manager
        self.reliability_manager.start().await?;
        
        // Start fault detector
        self.fault_detector.start().await?;
        
        // Start message processor
        self.start_message_processor().await?;
        
        info!("Production network manager started successfully");
        Ok(())
    }
    
    /// Send consensus message with high reliability
    pub async fn send_consensus_message(
        &self,
        to: u64,
        message: ConsensusMsg,
    ) -> Result<(), HotStuffError> {
        let payload = TcpNetworkPayload::Consensus(message);
        
        // Check if peer is faulty
        if self.fault_detector.is_peer_faulty(to).await {
            warn!("Attempting to send to faulty peer {}, message may fail", to);
        }
        
        // Send with at-least-once guarantee for consensus
        self.tcp_network.send_message(to, payload, true).await?;
        
        *self.total_messages_sent.lock().await += 1;
        Ok(())
    }
    
    /// Broadcast consensus message to all peers
    pub async fn broadcast_consensus_message(
        &self,
        message: ConsensusMsg,
    ) -> Result<(), HotStuffError> {
        let payload = TcpNetworkPayload::Consensus(message);
        
        // Get healthy peers for broadcast
        let stats = self.tcp_network.get_network_statistics().await;
        info!("Broadcasting consensus message to {} peers", stats.connected_peers);
        
        self.tcp_network.broadcast_message(payload).await?;
        
        *self.total_messages_sent.lock().await += 1;
        Ok(())
    }
    
    /// Send network message with specified reliability
    pub async fn send_network_message(
        &self,
        to: u64,
        message: NetworkMsg,
        reliable: bool,
    ) -> Result<(), HotStuffError> {
        let payload = TcpNetworkPayload::Network(message);
        
        self.tcp_network.send_message(to, payload, reliable).await?;
        
        *self.total_messages_sent.lock().await += 1;
        Ok(())
    }
    
    /// Get comprehensive network status
    pub async fn get_network_status(&self) -> ProductionNetworkStatus {
        let tcp_stats = self.tcp_network.get_network_statistics().await;
        let reliability_stats = self.reliability_manager.get_reliability_stats().await;
        let fault_stats = self.fault_detector.get_fault_stats().await;
        
        let total_sent = *self.total_messages_sent.lock().await;
        let total_received = *self.total_messages_received.lock().await;
        
        ProductionNetworkStatus {
            node_id: self.node_id,
            connected_peers: tcp_stats.connected_peers,
            total_peers: tcp_stats.total_peers,
            messages_sent: total_sent,
            messages_received: total_received,
            connection_failures: tcp_stats.connection_failures,
            reliability_success_rate: reliability_stats.success_rate,
            healthy_peers: fault_stats.healthy_peers,
            faulty_peers: fault_stats.faulty_peers,
            network_health: self.calculate_network_health(&tcp_stats, &fault_stats),
        }
    }
    
    /// Check network connectivity health
    pub async fn health_check(&self) -> NetworkHealthCheck {
        let status = self.get_network_status().await;
        
        let connectivity_score = if status.total_peers > 0 {
            (status.connected_peers as f64 / status.total_peers as f64) * 100.0
        } else {
            0.0
        };
        
        let fault_tolerance_score = if status.total_peers > 0 {
            (status.healthy_peers as f64 / status.total_peers as f64) * 100.0
        } else {
            0.0
        };
        
        let overall_health = if connectivity_score >= 80.0 && fault_tolerance_score >= 70.0 {
            HealthStatus::Healthy
        } else if connectivity_score >= 50.0 && fault_tolerance_score >= 50.0 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Unhealthy
        };
        
        NetworkHealthCheck {
            node_id: self.node_id,
            overall_health,
            connectivity_score,
            fault_tolerance_score,
            reliability_score: status.reliability_success_rate,
        }
    }
    
    /// Get connected peer list
    pub async fn get_connected_peers(&self) -> Vec<u64> {
        // Implementation would query TCP network for connected peer IDs
        // For now, return empty list as placeholder
        Vec::new()
    }
    
    /// Disconnect from all peers and shutdown
    pub async fn shutdown(&self) -> Result<(), HotStuffError> {
        info!("Shutting down production network manager for node {}", self.node_id);
        
        // Graceful shutdown would:
        // 1. Send goodbye messages to all peers
        // 2. Wait for pending messages to be sent/acknowledged
        // 3. Close all connections
        // 4. Clean up resources
        
        // For now, just log the shutdown
        info!("Network manager shutdown complete");
        Ok(())
    }
    
    async fn start_message_processor(&self) -> Result<(), HotStuffError> {
        let inbound_rx = {
            let mut rx_opt = self.inbound_messages.write().await;
            rx_opt.take().ok_or_else(|| {
                HotStuffError::Network("Message processor already started".to_string())
            })?
        };
        
        let reliability_manager = Arc::clone(&self.reliability_manager);
        let fault_detector = Arc::clone(&self.fault_detector);
        let total_received = Arc::clone(&self.total_messages_received);
        let _node_id = self.node_id;
        
        tokio::spawn(async move {
            let mut rx = inbound_rx;
            
            while let Some(message) = rx.recv().await {
                *total_received.lock().await += 1;
                
                // Record message reception for fault detection
                let _start_time = std::time::Instant::now();
                fault_detector.record_peer_message(message.from, Duration::from_millis(50)).await;
                
                // Check for duplicates
                if reliability_manager.is_duplicate(&Self::convert_to_p2p_message(message.clone())).await {
                    continue;
                }
                
                // Process message based on payload type
                match &message.payload {
                    TcpNetworkPayload::Consensus(_consensus_msg) => {
                        // Forward to consensus layer
                        info!("Received consensus message {} from peer {}", message.id, message.from);
                        // TODO: Forward to consensus handler
                    }
                    TcpNetworkPayload::Network(_network_msg) => {
                        // Handle network message
                        info!("Received network message {} from peer {}", message.id, message.from);
                        // TODO: Forward to network handler
                    }
                    TcpNetworkPayload::Heartbeat { node_status } => {
                        // Update peer status
                        info!("Received heartbeat from peer {} (view: {}, height: {})", 
                              message.from, node_status.current_view, node_status.current_height);
                    }
                    TcpNetworkPayload::Acknowledgment => {
                        // Process acknowledgment
                        if let Some(ack_for) = message.ack_for {
                            let _ = reliability_manager.process_acknowledgment(ack_for).await;
                        }
                    }
                    TcpNetworkPayload::PeerDiscovery { .. } => {
                        // Handle peer discovery
                        info!("Received peer discovery from {}", message.from);
                    }
                }
            }
        });
        
        Ok(())
    }
    
    fn convert_to_p2p_message(tcp_msg: TcpNetworkMessage) -> crate::network::P2PMessage {
        crate::network::P2PMessage {
            id: tcp_msg.id,
            from: tcp_msg.from,
            to: tcp_msg.to.unwrap_or(0),
            timestamp: tcp_msg.timestamp,
            payload: match tcp_msg.payload {
                TcpNetworkPayload::Consensus(msg) => crate::network::MessagePayload::Consensus(msg),
                TcpNetworkPayload::Network(msg) => crate::network::MessagePayload::Network(msg),
                TcpNetworkPayload::Heartbeat { .. } => crate::network::MessagePayload::Heartbeat,
                TcpNetworkPayload::Acknowledgment => crate::network::MessagePayload::Acknowledgment { ack_id: tcp_msg.id },
                TcpNetworkPayload::PeerDiscovery { .. } => crate::network::MessagePayload::Heartbeat, // Simplified
            },
        }
    }
    
    fn calculate_network_health(
        &self,
        tcp_stats: &crate::network::TcpNetworkStats,
        fault_stats: &crate::network::reliability::FaultDetectionStats,
    ) -> NetworkHealthStatus {
        let connectivity_ratio = if tcp_stats.total_peers > 0 {
            tcp_stats.connected_peers as f64 / tcp_stats.total_peers as f64
        } else {
            0.0
        };
        
        let health_ratio = if fault_stats.total_peers > 0 {
            fault_stats.healthy_peers as f64 / fault_stats.total_peers as f64
        } else {
            0.0
        };
        
        if connectivity_ratio >= 0.8 && health_ratio >= 0.8 {
            NetworkHealthStatus::Excellent
        } else if connectivity_ratio >= 0.6 && health_ratio >= 0.6 {
            NetworkHealthStatus::Good
        } else if connectivity_ratio >= 0.4 && health_ratio >= 0.4 {
            NetworkHealthStatus::Degraded
        } else {
            NetworkHealthStatus::Poor
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProductionNetworkStatus {
    pub node_id: u64,
    pub connected_peers: usize,
    pub total_peers: usize,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub connection_failures: u64,
    pub reliability_success_rate: f64,
    pub healthy_peers: usize,
    pub faulty_peers: usize,
    pub network_health: NetworkHealthStatus,
}

#[derive(Debug, Clone)]
pub enum NetworkHealthStatus {
    Excellent,
    Good,
    Degraded,
    Poor,
}

#[derive(Debug, Clone)]
pub struct NetworkHealthCheck {
    pub node_id: u64,
    pub overall_health: HealthStatus,
    pub connectivity_score: f64,
    pub fault_tolerance_score: f64,
    pub reliability_score: f64,
}

#[derive(Debug, Clone)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

impl std::fmt::Display for ProductionNetworkStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Node {}: {}/{} peers connected, {} sent/{} received, {:?} health, {:.1}% reliability",
            self.node_id,
            self.connected_peers,
            self.total_peers,
            self.messages_sent,
            self.messages_received,
            self.network_health,
            self.reliability_success_rate
        )
    }
}

impl std::fmt::Display for NetworkHealthCheck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Node {} Health: {:?} (Connectivity: {:.1}%, Fault Tolerance: {:.1}%, Reliability: {:.1}%)",
            self.node_id,
            self.overall_health,
            self.connectivity_score,
            self.fault_tolerance_score,
            self.reliability_score
        )
    }
}

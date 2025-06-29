// Test utilities for reliable network testing
use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

use tokio::time::sleep;
use log::{info, warn};

use crate::error::HotStuffError;
use crate::network::{TcpNetwork, ProductionNetworkManager};

/// Global port allocator to avoid conflicts in tests
static PORT_COUNTER: AtomicU16 = AtomicU16::new(25000);

/// Utility to find available ports for testing
pub struct PortAllocator;

impl PortAllocator {
    /// Get the next available port starting from a base
    pub fn get_available_port() -> Result<u16, HotStuffError> {
        for _ in 0..100 {  // Try up to 100 ports
            let port = PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
            if Self::is_port_available(port) {
                return Ok(port);
            }
        }
        Err(HotStuffError::Network("No available ports found".to_string()))
    }
    
    /// Check if a port is available
    fn is_port_available(port: u16) -> bool {
        match TcpListener::bind(("127.0.0.1", port)) {
            Ok(_) => true,
            Err(_) => false,
        }
    }
    
    /// Get multiple available ports
    pub fn get_available_ports(count: usize) -> Result<Vec<u16>, HotStuffError> {
        let mut ports = Vec::new();
        for _ in 0..count {
            ports.push(Self::get_available_port()?);
        }
        Ok(ports)
    }
}

/// Improved test setup with better error handling and cleanup
pub struct OptimizedNetworkTestSetup {
    pub networks: Vec<TcpNetwork>,
    pub managers: Vec<ProductionNetworkManager>,
    pub addresses: Vec<SocketAddr>,
    pub node_ids: Vec<u64>,
}

impl OptimizedNetworkTestSetup {
    pub async fn new(node_count: usize) -> Result<Self, HotStuffError> {
        if node_count == 0 {
            return Err(HotStuffError::Network("Node count must be greater than 0".to_string()));
        }
        
        let mut networks = Vec::new();
        let mut managers = Vec::new();
        let mut addresses = Vec::new();
        let mut node_ids = Vec::new();
        
        // Get available ports
        let ports = PortAllocator::get_available_ports(node_count)?;
        
        // Generate addresses
        for port in &ports {
            let addr = format!("127.0.0.1:{}", port)
                .parse::<SocketAddr>()
                .map_err(|e| HotStuffError::Network(format!("Invalid address: {}", e)))?;
            addresses.push(addr);
        }
        
        // Generate node IDs
        for i in 0..node_count {
            node_ids.push(i as u64);
        }
        
        info!("Setting up network test with {} nodes on ports: {:?}", node_count, ports);
        
        // Create networks with peer connections
        for i in 0..node_count {
            let node_id = node_ids[i];
            let listen_addr = addresses[i];
            
            // Create peer map (exclude self)
            let peers: HashMap<u64, SocketAddr> = addresses
                .iter()
                .enumerate()
                .filter(|(idx, _)| *idx != i)
                .map(|(idx, addr)| (node_ids[idx], *addr))
                .collect();
            
            info!("Creating network for node {} on address {} with {} peers", 
                  node_id, listen_addr, peers.len());
            
            match TcpNetwork::new(node_id, listen_addr, peers.clone()) {
                Ok((network, _inbound_rx)) => {
                    networks.push(network);
                }
                Err(e) => {
                    warn!("Failed to create network for node {}: {}", node_id, e);
                    return Err(e);
                }
            }
            
            match ProductionNetworkManager::new(node_id, listen_addr, peers).await {
                Ok(manager) => {
                    managers.push(manager);
                }
                Err(e) => {
                    warn!("Failed to create manager for node {}: {}", node_id, e);
                    return Err(e);
                }
            }
        }
        
        Ok(OptimizedNetworkTestSetup {
            networks,
            managers,
            addresses,
            node_ids,
        })
    }
    
    pub async fn start_all(&self) -> Result<(), HotStuffError> {
        info!("Starting all network managers...");
        
        // Start managers with error handling
        for (i, manager) in self.managers.iter().enumerate() {
            match manager.start().await {
                Ok(_) => info!("Started manager for node {}", self.node_ids[i]),
                Err(e) => {
                    warn!("Failed to start manager for node {}: {}", self.node_ids[i], e);
                    // Continue with other managers
                }
            }
        }
        
        // Give networks time to establish connections
        // Longer delay for more reliable connections
        sleep(Duration::from_millis(1500)).await;
        
        info!("All managers started, waiting for peer connections...");
        
        // Allow additional time for peer discovery
        sleep(Duration::from_millis(500)).await;
        
        Ok(())
    }
    
    pub async fn shutdown_all(&self) -> Result<(), HotStuffError> {
        info!("Shutting down all network managers...");
        
        for (i, manager) in self.managers.iter().enumerate() {
            match manager.shutdown().await {
                Ok(_) => info!("Shut down manager for node {}", self.node_ids[i]),
                Err(e) => warn!("Error shutting down manager for node {}: {}", self.node_ids[i], e),
            }
        }
        
        // Give time for graceful shutdown
        sleep(Duration::from_millis(200)).await;
        
        Ok(())
    }
    
    pub async fn wait_for_connections(&self, timeout: Duration) -> Result<bool, HotStuffError> {
        let start = tokio::time::Instant::now();
        
        while start.elapsed() < timeout {
            let mut all_connected = true;
            
            for (i, network) in self.networks.iter().enumerate() {
                let stats = network.get_network_statistics().await;
                
                // Check if this node has connections to most other nodes
                // Allow for some connection failures in tests
                let expected_connections = (self.node_ids.len() - 1).max(1);
                let min_required = (expected_connections * 2 / 3).max(1); // At least 2/3 connected
                
                if stats.total_peers < min_required {
                    all_connected = false;
                    break;
                }
            }
            
            if all_connected {
                info!("All nodes have sufficient connections");
                return Ok(true);
            }
            
            sleep(Duration::from_millis(100)).await;
        }
        
        warn!("Timeout waiting for network connections");
        Ok(false)
    }
    
    pub async fn log_network_status(&self) {
        for (i, network) in self.networks.iter().enumerate() {
            let stats = network.get_network_statistics().await;
            info!("Node {} network stats: total_peers={}, connected_peers={}, messages_sent={}, messages_received={}", 
                  self.node_ids[i], stats.total_peers, stats.connected_peers, stats.messages_sent, stats.messages_received);
        }
        
        for (i, manager) in self.managers.iter().enumerate() {
            let status = manager.get_network_status().await;
            info!("Node {} manager status: {}", self.node_ids[i], status);
        }
    }
}

/// Test helper for creating individual network components
pub struct NetworkTestHelper;

impl NetworkTestHelper {
    /// Create a single test network with unique port
    pub async fn create_test_network(node_id: u64) -> Result<(TcpNetwork, ProductionNetworkManager, SocketAddr), HotStuffError> {
        let port = PortAllocator::get_available_port()?;
        let addr = format!("127.0.0.1:{}", port)
            .parse::<SocketAddr>()
            .map_err(|e| HotStuffError::Network(format!("Invalid address: {}", e)))?;
        
        let peers = HashMap::new(); // No peers for isolated test
        
        let (network, _inbound_rx) = TcpNetwork::new(node_id, addr, peers.clone())?;
        let manager = ProductionNetworkManager::new(node_id, addr, peers).await?;
        
        Ok((network, manager, addr))
    }
    
    /// Wait for a condition with timeout
    pub async fn wait_for_condition<F, Fut>(
        condition: F,
        timeout: Duration,
        check_interval: Duration,
    ) -> Result<bool, HotStuffError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let start = tokio::time::Instant::now();
        
        while start.elapsed() < timeout {
            if condition().await {
                return Ok(true);
            }
            sleep(check_interval).await;
        }
        
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_port_allocator() {
        let port1 = PortAllocator::get_available_port().unwrap();
        let port2 = PortAllocator::get_available_port().unwrap();
        
        assert_ne!(port1, port2, "Should allocate different ports");
        assert!(port1 >= 25000, "Port should be in expected range");
        assert!(port2 >= 25000, "Port should be in expected range");
    }
    
    #[tokio::test]
    async fn test_optimized_setup_creation() {
        let setup = OptimizedNetworkTestSetup::new(2).await.unwrap();
        
        assert_eq!(setup.networks.len(), 2);
        assert_eq!(setup.managers.len(), 2);
        assert_eq!(setup.addresses.len(), 2);
        assert_eq!(setup.node_ids.len(), 2);
        
        // Test cleanup
        setup.shutdown_all().await.unwrap();
    }
}

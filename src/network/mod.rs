pub mod client;
pub mod server;
pub mod transport;
pub mod p2p;
pub mod tcp_network;
pub mod reliability;
pub mod production_manager;
pub mod production_tcp;

#[cfg(test)]
pub mod test_utils;
#[cfg(test)]
pub mod tests;
#[cfg(test)]
pub mod simple_tests;
#[cfg(test)]
pub mod optimized_tests;
#[cfg(test)]
pub mod multinode_tests;

pub use client::NetworkClient;
pub use server::NetworkServer;
pub use p2p::{P2PNetwork, P2PMessage, MessagePayload, NetworkStats};
pub use tcp_network::{TcpNetwork, TcpNetworkStats, NetworkPayload as TcpNetworkPayload};
pub use reliability::{NetworkReliabilityManager, NetworkFaultDetector, DeliveryGuarantee};
pub use production_manager::{ProductionNetworkManager, ProductionNetworkStatus, NetworkHealthCheck};
pub use production_tcp::{ProductionP2PNetwork, NetworkConfig as ProductionNetworkConfig, NetworkStats as ProductionNetworkStats};

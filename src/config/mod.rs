use std::collections::HashMap;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::error::HotStuffError;

/// Complete configuration for a HotStuff-2 node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    // Node identity
    pub node_id: u64,
    pub listen_addr: String,
    pub peers: Vec<PeerConfig>,
    
    // Consensus parameters
    pub consensus: ConsensusConfig,
    
    // Network configuration
    pub network: NetworkConfig,
    
    // Storage configuration
    pub storage: StorageConfig,
    
    // Metrics configuration
    pub metrics: MetricsConfig,
    
    // Crypto configuration
    pub crypto: CryptoConfig,
    
    // Logging configuration
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConfig {
    pub node_id: u64,
    pub address: String,
    pub public_key: Option<String>, // Hex-encoded public key
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusConfig {
    // Timing parameters
    pub base_timeout_ms: u64,
    pub timeout_multiplier: f64,
    pub max_block_size: usize,
    pub max_transactions_per_block: usize,
    
    // Batching parameters
    pub max_batch_size: usize,
    pub batch_timeout_ms: u64,
    
    // Safety parameters
    pub byzantine_fault_tolerance: u64, // f in n = 3f + 1
    pub enable_pipelining: bool,
    pub pipeline_depth: usize,
    
    // Optimistic responsiveness
    pub optimistic_mode: bool,
    pub fast_path_timeout_ms: u64,
    pub optimistic_threshold: f64, // Confidence threshold for optimistic execution
    pub synchrony_detection_window: usize,
    pub max_network_delay_ms: u64,
    pub latency_variance_threshold_ms: u64,
    
    // View change parameters
    pub view_change_timeout_ms: u64,
    pub max_view_changes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    // Network implementation selection
    pub use_p2p_network: bool, // If true, use production P2P; if false, use legacy networking
    
    // Connection parameters
    pub max_peers: usize,
    pub connection_timeout_ms: u64,
    pub heartbeat_interval_ms: u64,
    pub max_message_size: usize,
    
    // Retry parameters
    pub max_retries: u32,
    pub retry_delay_ms: u64,
    pub exponential_backoff: bool,
    
    // Buffer sizes
    pub send_buffer_size: usize,
    pub receive_buffer_size: usize,
    
    // Security
    pub enable_tls: bool,
    pub tls_cert_path: Option<PathBuf>,
    pub tls_key_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub data_dir: PathBuf,
    pub storage_type: StorageType,
    
    // RocksDB specific settings
    pub rocksdb: RocksDBConfig,
    
    // Memory settings
    pub memory_limit_mb: Option<u64>,
    
    // Persistence settings
    pub enable_wal: bool,
    pub sync_writes: bool,
    pub compaction_interval_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageType {
    Memory,
    RocksDB,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RocksDBConfig {
    pub max_open_files: i32,
    pub write_buffer_size: usize,
    pub max_write_buffer_number: i32,
    pub compression: String, // "none", "snappy", "lz4", "zlib", "zstd"
    pub block_cache_size: usize,
    pub bloom_filter_bits: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub port: u16,
    pub endpoint: String,
    pub collection_interval_ms: u64,
    pub retention_days: u32,
    
    // Prometheus integration
    pub prometheus_enabled: bool,
    pub prometheus_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoConfig {
    pub signature_scheme: SignatureScheme,
    pub key_size: usize,
    pub use_threshold_signatures: bool,
    pub threshold: Option<usize>,
    
    // Key management
    pub private_key_path: Option<PathBuf>,
    pub public_key_path: Option<PathBuf>,
    pub keystore_path: Option<PathBuf>,
    
    // Security
    pub enable_key_rotation: bool,
    pub key_rotation_interval_hours: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignatureScheme {
    Ed25519,
    Secp256k1,
    BLS,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub output: LogOutput,
    pub format: LogFormat,
    pub file_rotation: FileRotationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogOutput {
    Console,
    File(PathBuf),
    Both { file: PathBuf },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogFormat {
    Plain,
    Json,
    Structured,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRotationConfig {
    pub max_size_mb: u64,
    pub max_files: u32,
    pub rotation_interval_hours: u64,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            node_id: 0,
            listen_addr: "127.0.0.1:8000".to_string(),
            peers: Vec::new(),
            consensus: ConsensusConfig::default(),
            network: NetworkConfig::default(),
            storage: StorageConfig::default(),
            metrics: MetricsConfig::default(),
            crypto: CryptoConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            base_timeout_ms: 1000,
            timeout_multiplier: 1.5,
            max_block_size: 1024 * 1024, // 1MB
            max_transactions_per_block: 1000,
            max_batch_size: 100,
            batch_timeout_ms: 50,
            byzantine_fault_tolerance: 1, // Tolerates 1 fault in 4 nodes
            enable_pipelining: true,
            pipeline_depth: 3,
            optimistic_mode: true,
            fast_path_timeout_ms: 100,
            optimistic_threshold: 0.8,
            synchrony_detection_window: 50,
            max_network_delay_ms: 100,
            latency_variance_threshold_ms: 50,
            view_change_timeout_ms: 5000,
            max_view_changes: 10,
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            use_p2p_network: false, // Default to legacy networking for compatibility
            max_peers: 100,
            connection_timeout_ms: 5000,
            heartbeat_interval_ms: 1000,
            max_message_size: 10 * 1024 * 1024, // 10MB
            max_retries: 3,
            retry_delay_ms: 1000,
            exponential_backoff: true,
            send_buffer_size: 64 * 1024,
            receive_buffer_size: 64 * 1024,
            enable_tls: false,
            tls_cert_path: None,
            tls_key_path: None,
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./data"),
            storage_type: StorageType::RocksDB,
            rocksdb: RocksDBConfig::default(),
            memory_limit_mb: Some(1024), // 1GB
            enable_wal: true,
            sync_writes: false,
            compaction_interval_ms: 300000, // 5 minutes
        }
    }
}

impl Default for RocksDBConfig {
    fn default() -> Self {
        Self {
            max_open_files: 1000,
            write_buffer_size: 64 * 1024 * 1024, // 64MB
            max_write_buffer_number: 3,
            compression: "snappy".to_string(),
            block_cache_size: 256 * 1024 * 1024, // 256MB
            bloom_filter_bits: 10,
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 9090,
            endpoint: "/metrics".to_string(),
            collection_interval_ms: 1000,
            retention_days: 7,
            prometheus_enabled: false,
            prometheus_port: 9091,
        }
    }
}

impl Default for CryptoConfig {
    fn default() -> Self {
        Self {
            signature_scheme: SignatureScheme::Ed25519,
            key_size: 256,
            use_threshold_signatures: false,
            threshold: None,
            private_key_path: None,
            public_key_path: None,
            keystore_path: None,
            enable_key_rotation: false,
            key_rotation_interval_hours: 24 * 7, // Weekly
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            output: LogOutput::Console,
            format: LogFormat::Plain,
            file_rotation: FileRotationConfig::default(),
        }
    }
}

impl Default for FileRotationConfig {
    fn default() -> Self {
        Self {
            max_size_mb: 100,
            max_files: 10,
            rotation_interval_hours: 24,
        }
    }
}

impl NodeConfig {
    /// Load configuration from a file
    pub fn load_from_file(path: &std::path::Path) -> Result<Self, HotStuffError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| HotStuffError::Configuration(format!("Failed to read config file: {}", e)))?;
        
        // Try TOML first, then JSON
        if path.extension() == Some(std::ffi::OsStr::new("toml")) {
            toml::from_str(&content)
                .map_err(|e| HotStuffError::Configuration(format!("Failed to parse TOML config: {}", e)))
        } else {
            serde_json::from_str(&content)
                .map_err(|e| HotStuffError::Configuration(format!("Failed to parse JSON config: {}", e)))
        }
    }

    /// Save configuration to a file
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<(), HotStuffError> {
        let content = if path.extension() == Some(std::ffi::OsStr::new("toml")) {
            toml::to_string_pretty(self)
                .map_err(|e| HotStuffError::Configuration(format!("Failed to serialize to TOML: {}", e)))?
        } else {
            serde_json::to_string_pretty(self)
                .map_err(|e| HotStuffError::Configuration(format!("Failed to serialize to JSON: {}", e)))?
        };
        
        std::fs::write(path, content)
            .map_err(|e| HotStuffError::Configuration(format!("Failed to write config file: {}", e)))?;
        
        Ok(())
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), HotStuffError> {
        // Validate node ID
        if self.peers.iter().any(|p| p.node_id == self.node_id) {
            return Err(HotStuffError::Configuration(
                "Node ID cannot be in the peers list".to_string()
            ));
        }

        // Validate BFT parameters
        let total_nodes = self.peers.len() as u64 + 1; // +1 for this node
        let required_nodes = 3 * self.consensus.byzantine_fault_tolerance + 1;
        if total_nodes < required_nodes {
            return Err(HotStuffError::Configuration(
                format!("Need at least {} nodes for f={} Byzantine faults, but only have {}", 
                       required_nodes, self.consensus.byzantine_fault_tolerance, total_nodes)
            ));
        }

        // Validate threshold signatures
        if self.crypto.use_threshold_signatures {
            let threshold = self.crypto.threshold.unwrap_or(0);
            if threshold == 0 || threshold > total_nodes as usize {
                return Err(HotStuffError::Configuration(
                    format!("Invalid threshold {} for {} nodes", threshold, total_nodes)
                ));
            }
        }

        // Validate paths exist
        if let Some(ref path) = self.crypto.private_key_path {
            if !path.exists() {
                return Err(HotStuffError::Configuration(
                    format!("Private key file does not exist: {:?}", path)
                ));
            }
        }

        Ok(())
    }

    /// Get peer configurations as a HashMap
    pub fn peer_map(&self) -> HashMap<u64, PeerConfig> {
        self.peers.iter()
            .map(|p| (p.node_id, p.clone()))
            .collect()
    }

    /// Get the total number of nodes (including this one)
    pub fn total_nodes(&self) -> usize {
        self.peers.len() + 1
    }

    /// Get the consensus timeout as Duration
    pub fn base_timeout(&self) -> Duration {
        Duration::from_millis(self.consensus.base_timeout_ms)
    }

    /// Get the view change timeout as Duration
    pub fn view_change_timeout(&self) -> Duration {
        Duration::from_millis(self.consensus.view_change_timeout_ms)
    }

    /// Create default config for testing
    pub fn default_for_testing() -> Self {
        Self {
            node_id: 0,
            listen_addr: "127.0.0.1:8000".to_string(),
            peers: Vec::new(),
            consensus: ConsensusConfig {
                base_timeout_ms: 5000,
                timeout_multiplier: 1.5,
                max_block_size: 1024 * 1024,
                max_transactions_per_block: 1000,
                max_batch_size: 100,
                batch_timeout_ms: 1000,
                byzantine_fault_tolerance: 1,
                enable_pipelining: true,
                pipeline_depth: 3,
                optimistic_mode: true,
                fast_path_timeout_ms: 1000,
                optimistic_threshold: 0.66,
                synchrony_detection_window: 10,
                max_network_delay_ms: 1000,
                latency_variance_threshold_ms: 100,
                view_change_timeout_ms: 10000,
                max_view_changes: 10,
            },
            network: NetworkConfig {
                use_p2p_network: false,
                max_peers: 100,
                connection_timeout_ms: 10000,
                heartbeat_interval_ms: 5000,
                max_message_size: 1024 * 1024,
                max_retries: 3,
                retry_delay_ms: 1000,
                exponential_backoff: true,
                send_buffer_size: 64 * 1024,
                receive_buffer_size: 64 * 1024,
                enable_tls: false,
                tls_cert_path: None,
                tls_key_path: None,
            },
            storage: StorageConfig {
                data_dir: "/tmp/hotstuff_test".into(),
                storage_type: StorageType::Memory,
                rocksdb: RocksDBConfig {
                    max_open_files: 1000,
                    write_buffer_size: 64 * 1024 * 1024,
                    max_write_buffer_number: 3,
                    compression: "snappy".to_string(),
                    block_cache_size: 64 * 1024 * 1024,
                    bloom_filter_bits: 10,
                },
                memory_limit_mb: Some(256),
                enable_wal: false,
                sync_writes: false,
                compaction_interval_ms: 300000,
            },
            metrics: MetricsConfig {
                enabled: true,
                port: 9090,
                endpoint: "/metrics".to_string(),
                collection_interval_ms: 1000,
                retention_days: 7,
                prometheus_enabled: false,
                prometheus_port: 9091,
            },
            crypto: CryptoConfig {
                signature_scheme: SignatureScheme::Ed25519,
                key_size: 256,
                use_threshold_signatures: true,
                threshold: Some(2),
                private_key_path: None,
                public_key_path: None,
                keystore_path: None,
                enable_key_rotation: false,
                key_rotation_interval_hours: 24,
            },
            logging: LoggingConfig {
                level: "debug".to_string(),
                output: LogOutput::Console,
                format: LogFormat::Plain,
                file_rotation: FileRotationConfig::default(),
            },
        }
    }

    /// Create default config for testing with specific node ID
    pub fn default_for_testing_with_id(node_id: u64) -> Self {
        let mut config = Self::default_for_testing();
        config.node_id = node_id;
        config.listen_addr = format!("127.0.0.1:{}", 8000 + node_id);
        config
    }
}

/// Alias for the complete HotStuff-2 configuration
pub type HotStuffConfig = NodeConfig;

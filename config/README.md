# HotStuff-2 Configuration Management

**Centralized configuration system** for all HotStuff-2 consensus components, based on the HotStuff-2 paper specifications and production requirements.

## 🎯 Overview

The configuration module provides a unified, type-safe configuration system for all HotStuff-2 components, ensuring consistency across the entire consensus protocol implementation.

### Design Principles

1. **Type Safety**: All configurations are strongly typed with validation
2. **Modularity**: Each component has its own configuration namespace
3. **Default Values**: Sensible defaults based on HotStuff-2 paper recommendations
4. **Environment Flexibility**: Support for development, testing, and production environments
5. **Validation**: Cross-component validation to ensure configuration consistency

## 🏗️ Architecture

### Configuration Hierarchy

```
HotStuffConfig (Master Configuration)
├── NodeConfig          ← Node runtime and API settings
├── ConsensusConfig     ← HotStuff-2 protocol parameters
├── NetworkConfig       ← P2P networking and discovery
├── CryptoConfig        ← Cryptographic schemes and keys
├── StorageConfig       ← Blockchain and state storage
├── ValidatorConfig     ← Validator-specific settings
├── MempoolConfig       ← Transaction pool management
├── ExecutorConfig      ← Transaction execution engine
├── RpcConfig           ← External RPC interfaces
├── SyncConfig          ← Node synchronization settings
└── MetricsConfig       ← Monitoring and observability
```

### Component Integration

```
┌─────────────────────────────────────────┐
│          Application Layer              │
├─────────────────────────────────────────┤
│         HotStuffConfig Loader           │  ← config/lib.rs
├─────────────────────────────────────────┤
│ Node │Consensus│Network│Crypto│Storage  │  ← Individual configs
│Config│ Config  │Config │Config│Config   │
├─────────────────────────────────────────┤
│        TOML Configuration Files         │  ← File-based configs
└─────────────────────────────────────────┘
```

## 📋 Configuration Components

### **1. ConsensusConfig** - HotStuff-2 Protocol Parameters

**Purpose**: Core consensus algorithm configuration based on HotStuff-2 paper specifications.

```rust
pub struct ConsensusConfig {
    pub view_timeout_ms: u64,           // View change timeout
    pub max_concurrent_views: u32,      // Multi-view pipeline
    pub max_block_size: usize,          // Block size limits
    pub safety_threshold: f64,          // Byzantine fault tolerance (2/3+1)
    pub pacemaker: PacemakerConfig,     // View progression control
    // ...
}
```

**Key Features**:
- View timeout management with exponential backoff
- Safety and liveness threshold configuration
- Block size and transaction limits
- Pacemaker settings for view progression

### **2. NetworkConfig** - P2P Communication

**Purpose**: Network layer configuration for HotStuff-2 node communication.

```rust
pub struct NetworkConfig {
    pub bind_address: SocketAddr,       // Local binding address
    pub max_peers: usize,               // Peer connection limits
    pub protocol: ProtocolConfig,       // Protocol-specific settings
    pub discovery: DiscoveryConfig,     // Peer discovery
    pub security: SecurityConfig,       // TLS and authentication
}
```

**Key Features**:
- Peer discovery and bootstrap configuration
- Message compression and protocol optimization
- TLS encryption and security settings
- Connection management and heartbeat

### **3. CryptoConfig** - Cryptographic Configuration

**Purpose**: Cryptographic scheme configuration for signatures, hashing, and key management.

```rust
pub struct CryptoConfig {
    pub signature: SignatureConfig,              // Digital signatures
    pub threshold_signature: ThresholdSignatureConfig, // Committee signatures
    pub hash: HashConfig,                        // Hash functions
    pub key_management: KeyManagementConfig,     // Key storage and rotation
}
```

**Key Features**:
- Support for Ed25519, BLS12-381, and other signature schemes
- Threshold signature configuration for committee operations
- Hash algorithm selection (SHA-256, Blake2, etc.)
- Key rotation and storage backend configuration

### **4. StorageConfig** - Blockchain Storage

**Purpose**: Configuration for blockchain data storage, state management, and persistence.

```rust
pub struct StorageConfig {
    pub backend: StorageBackend,        // Storage backend selection
    pub database: DatabaseConfig,       // Database settings
    pub block_storage: BlockStorageConfig, // Block-specific storage
    pub state_storage: StateStorageConfig, // State trie storage
}
```

**Key Features**:
- Multiple storage backends (Memory, File, Database, Distributed)
- Block compression and indexing
- State trie optimization and caching
- Data pruning and archival policies

### **5. NodeConfig** - Node Runtime Configuration

**Purpose**: Node-level configuration for runtime, resources, and API services.

```rust
pub struct NodeConfig {
    pub node_id: String,                // Unique node identifier
    pub node_type: NodeType,            // Validator/Replica/Archive/Light
    pub runtime: RuntimeConfig,         // Thread and async settings
    pub resources: ResourceConfig,      // Resource limits
    pub api_server: ApiServerConfig,    // API server settings
}
```

**Key Features**:
- Node type specification (Validator, Replica, Archive, Light)
- Runtime optimization (thread pools, async configuration)
- Resource limits (memory, CPU, disk, bandwidth)
- API server configuration with rate limiting

### **6. ValidatorConfig** - Validator-Specific Settings

**Purpose**: Configuration for consensus validators, including identity, staking, and slashing protection.

```rust
pub struct ValidatorConfig {
    pub identity: ValidatorIdentity,    // Validator keys and identity
    pub consensus: ValidatorConsensusConfig, // Voting and proposal settings
    pub committee: CommitteeConfig,     // Committee membership
    pub incentives: IncentiveConfig,    // Rewards and penalties
    pub slashing_protection: SlashingProtectionConfig, // Safety mechanisms
}
```

**Key Features**:
- Validator identity and key management
- Voting power and consensus participation
- Committee selection and rotation
- Reward distribution and slashing protection

### **7. ClientConfig** - Client SDK Configuration

**Purpose**: Configuration for HotStuff-2 client SDK and CLI tools.

```rust
pub struct ClientConfig {
    pub endpoint: String,               // Server endpoint
    pub timeout_ms: u64,                // Request timeout
    pub retry_attempts: u32,            // Retry configuration
    pub connection: ConnectionConfig,   // Connection pooling
    pub security: ClientSecurityConfig, // TLS and authentication
}
```

**Key Features**:
- Client endpoint and timeout configuration
- Connection pooling and keep-alive settings
- TLS verification and client authentication
- Request logging and structured output

## 🚀 Usage Examples

### Loading Configuration from File

```rust
use hotstuff2_config::HotStuffConfig;

// Load complete configuration
let config = HotStuffConfig::load_from_file("./config/node.toml")?;

// Access specific components
let consensus_timeout = config.consensus.view_timeout_ms;
let max_peers = config.network.max_peers;
let storage_backend = &config.storage.backend;
```

### Creating Default Configuration

```rust
use hotstuff2_config::*;

// Create with defaults
let config = HotStuffConfig {
    node: NodeConfig::default(),
    consensus: ConsensusConfig::default(),
    network: NetworkConfig::default(),
    crypto: CryptoConfig::default(),
    storage: StorageConfig::default(),
    validator: ValidatorConfig::default(),
};

// Validate configuration
config.validate()?;
```

### Component-Specific Usage

```rust
use hotstuff2_config::{ConsensusConfig, NetworkConfig};

// Use individual configs
let consensus_config = ConsensusConfig {
    view_timeout_ms: 15000,
    max_block_size: 2 * 1024 * 1024, // 2MB
    safety_threshold: 0.67,
    ..Default::default()
};

let network_config = NetworkConfig {
    bind_address: "0.0.0.0:8080".parse().unwrap(),
    max_peers: 200,
    ..Default::default()
};
```

## 📁 Configuration Files

### Example TOML Configuration

```toml
# node.toml - Complete HotStuff-2 node configuration

[node]
node_id = "validator-001"
node_type = "Validator"

[node.runtime]
worker_threads = 8
async_runtime = true
enable_profiling = false

[consensus]
view_timeout_ms = 10000
max_concurrent_views = 3
max_block_size = 1048576
safety_threshold = 0.67

[consensus.pacemaker]
base_timeout_ms = 1000
timeout_multiplier = 1.5
exponential_backoff = true

[network]
bind_address = "0.0.0.0:8080"
max_peers = 100
max_message_size = 10485760

[network.discovery]
bootstrap_peers = [
    "192.168.1.10:8080",
    "192.168.1.11:8080"
]

[crypto]
[crypto.signature]
algorithm = "ed25519"
enable_batching = true

[storage]
[storage.backend]
File = { data_dir = "./data" }

[validator]
[validator.identity]
name = "validator-001"
public_key = "..."
private_key_path = "./keys/validator.key"
```

## 🔧 Integration with Client Module

The `ClientConfig` has been moved from the client module to this centralized configuration system:

```rust
// Before (in client/src/lib.rs)
pub struct ClientConfig { ... }

// After (in config/src/client.rs)
pub struct ClientConfig { ... }

// Usage in client module
use hotstuff2_config::ClientConfig;
```

**Migration Benefits**:
- ✅ Centralized configuration management
- ✅ Consistent validation across all components
- ✅ Unified TOML file format
- ✅ Type-safe configuration access

## 🛠️ Implementation Status

🚧 **Framework Phase**: This module provides architectural configuration definitions for HotStuff-2 components.

**Current State**:
- ✅ Configuration structure design
- ✅ Type-safe configuration definitions
- ✅ Default value specifications
- ✅ TOML serialization support
- ⏳ Configuration validation implementation
- ⏳ Environment variable overrides
- ⏳ Configuration migration tools

## 🔗 Dependencies

- **serde**: Serialization and deserialization
- **toml**: TOML configuration file parsing
- **thiserror**: Error handling
- **num_cpus**: Runtime thread detection

## 📖 References

- **HotStuff-2 Paper**: Consensus algorithm specifications
- **Production Best Practices**: Enterprise deployment considerations
- **Rust Configuration Patterns**: Type-safe configuration management

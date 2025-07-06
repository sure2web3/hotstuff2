# HotStuff-2 Storage Layer

**High-performance, consensus-optimized storage framework** designed specifically for HotStuff-2's data access patterns and consistency requirements.

## 🎯 Design Philosophy

The HotStuff-2 storage layer provides **minimal, consensus-focused interfaces** that abstract away storage complexity while maintaining optimal performance for consensus operations.

### Core Principles

1. **Consensus-Optimized**: Designed for HotStuff-2's specific data patterns (blocks, votes, consensus state)
2. **Minimal Interface**: Only essential operations needed by consensus protocol
3. **Performance-First**: Optimized for the critical path of consensus operations
4. **Pluggable Backends**: Abstract interface supporting multiple storage implementations
5. **Safety-Aware**: Consistency guarantees aligned with consensus safety requirements

## 🏗️ Architecture Overview

### Storage Abstraction Layers

```
┌─────────────────────────────────────────┐
│         HotStuff-2 Consensus             │
├─────────────────────────────────────────┤
│  Block Store  │ Vote Store │ State Store│  ← Domain-Specific Interfaces
├─────────────────────────────────────────┤
│         Core Storage Interface          │  ← Unified Storage API
├─────────────────────────────────────────┤
│ Memory │ Persistent │ Distributed │ ... │  ← Pluggable Backends
└─────────────────────────────────────────┘
```

## 🔧 Core Storage Interface

### `HotStuffStorage` Trait

**Purpose**: Unified storage abstraction for all HotStuff-2 data operations.

```rust
#[async_trait]
pub trait HotStuffStorage: Send + Sync + Clone {
    // Basic Operations
    async fn get(&self, key: &[u8]) -> StorageResult<Option<Vec<u8>>>;
    async fn put(&self, key: &[u8], value: &[u8]) -> StorageResult<()>;
    async fn delete(&self, key: &[u8]) -> StorageResult<()>;
    async fn exists(&self, key: &[u8]) -> StorageResult<bool>;
    
    // Consensus-Specific Operations
    async fn get_keys_with_prefix(&self, prefix: &[u8]) -> StorageResult<Vec<Vec<u8>>>;
    async fn batch_write(&self, operations: Vec<WriteOperation>) -> StorageResult<()>;
    async fn clear(&self) -> StorageResult<()>;
    
    // Metrics & Monitoring
    async fn storage_stats(&self) -> StorageResult<StorageStats>;
}
```

**Key Design Decisions**:
- **Async-first**: Non-blocking I/O for consensus performance
- **Thread-safe**: Safe concurrent access from multiple consensus threads
- **Prefix queries**: Support for efficient range operations (e.g., all votes for a block)
- **Batch operations**: Atomic multi-key operations for consistency

## 🗃️ Domain-Specific Storage Interfaces

### Block Storage (`BlockStore`)

**Purpose**: Efficient storage and retrieval of consensus blocks.

```rust
pub struct BlockStore<S: HotStuffStorage> {
    storage: S,
}

impl<S: HotStuffStorage> BlockStore<S> {
    // Block Operations
    async fn store_block(&self, block: &Block) -> StorageResult<()>;
    async fn get_block(&self, hash: &Hash) -> StorageResult<Option<Block>>;
    async fn get_block_by_height(&self, height: u64) -> StorageResult<Option<Block>>;
    
    // Chain Operations
    async fn get_latest_block(&self) -> StorageResult<Option<Block>>;
    async fn get_genesis_block(&self) -> StorageResult<Option<Block>>;
    async fn get_block_range(&self, start: u64, end: u64) -> StorageResult<Vec<Block>>;
    
    // Cleanup Operations
    async fn prune_blocks_before(&self, height: u64) -> StorageResult<()>;
}
```

**Key Features**:
- Height-based and hash-based indexing
- Efficient range queries for chain traversal
- Pruning support for long-running deployments

### Vote Storage (`VoteStore`)

**Purpose**: Efficient aggregation and retrieval of consensus votes.

```rust
pub struct VoteStore<S: HotStuffStorage> {
    storage: S,
}

impl<S: HotStuffStorage> VoteStore<S> {
    // Vote Operations
    async fn store_vote(&self, vote: &Vote) -> StorageResult<()>;
    async fn get_votes_for_block(&self, block_hash: &Hash) -> StorageResult<Vec<Vote>>;
    async fn get_vote(&self, block_hash: &Hash, replica_id: &ReplicaId) -> StorageResult<Option<Vote>>;
    
    // Aggregation
    async fn count_votes_for_block(&self, block_hash: &Hash) -> StorageResult<usize>;
    async fn has_quorum_for_block(&self, block_hash: &Hash, threshold: usize) -> StorageResult<bool>;
    
    // Cleanup
    async fn prune_votes_before_view(&self, view: u64) -> StorageResult<()>;
}
```

**Key Features**:
- Efficient vote aggregation queries
- Quorum checking without full vote retrieval
- View-based pruning for memory management

### Consensus State Storage (`StateStore`)

**Purpose**: Persistent storage for consensus protocol state.

```rust
pub struct StateStore<S: HotStuffStorage> {
    storage: S,
}

impl<S: HotStuffStorage> StateStore<S> {
    // View Management
    async fn get_current_view(&self) -> StorageResult<u64>;
    async fn set_current_view(&self, view: u64) -> StorageResult<()>;
    
    // Safety State
    async fn get_locked_block(&self) -> StorageResult<Option<Hash>>;
    async fn set_locked_block(&self, block_hash: Option<&Hash>) -> StorageResult<()>;
    
    async fn get_committed_block(&self) -> StorageResult<Option<Hash>>;
    async fn set_committed_block(&self, block_hash: &Hash) -> StorageResult<()>;
    
    // Configuration State
    async fn get_validator_set(&self) -> StorageResult<Option<ValidatorSet>>;
    async fn set_validator_set(&self, validators: &ValidatorSet) -> StorageResult<()>;
}
```

**Key Features**:
- Atomic state updates for safety guarantees
- Recovery support for node restarts
- Configuration persistence across sessions

## 🚀 Storage Backend Implementations

### Memory Storage (`MemoryStorage`)

**Purpose**: High-performance in-memory storage for development and testing.

- **Use Cases**: Development, testing, ephemeral deployments
- **Performance**: O(log n) operations with `BTreeMap`
- **Thread Safety**: `RwLock` for concurrent access
- **Persistence**: None (data lost on restart)

### Persistent Memory Storage (`PersistentMemoryStorage`)

**Purpose**: Memory storage with periodic file persistence.

- **Use Cases**: Production deployments requiring persistence
- **Performance**: Memory-speed reads, async persistence writes
- **Durability**: Configurable persistence intervals
- **Recovery**: Automatic recovery on startup

### Distributed Storage (`DistributedStorage`)

**Purpose**: Fault-tolerant distributed storage for multi-node deployments.

- **Use Cases**: Large-scale production networks
- **Consistency**: Configurable consistency levels
- **Replication**: Automatic data replication
- **Partition Tolerance**: Continues operation during network partitions

## 📊 Key Naming Conventions

Consistent key prefixes for different data types:

- `block:{hash}` - Block storage by hash
- `block_height:{height}` - Block storage by height  
- `vote:{block_hash}:{replica_id}` - Individual votes
- `view:current` - Current consensus view
- `state:locked` - Currently locked block
- `state:committed` - Latest committed block
- `config:validators` - Current validator set

## 🔒 Consistency Guarantees

### Safety-Critical Operations

- **Block commits**: Atomic block + state updates
- **View changes**: Consistent view progression
- **Vote aggregation**: Atomic vote storage and counting

### Performance Optimizations

- **Read-heavy workload**: Optimized for frequent block/vote queries
- **Batch operations**: Minimize I/O for multi-key operations
- **Lazy persistence**: Memory-first with async durability

## 🛠️ Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Key not found: {key}")]
    NotFound { key: String },
    
    #[error("Storage backend error: {source}")]
    Backend { source: Box<dyn std::error::Error + Send + Sync> },
    
    #[error("Serialization error: {message}")]
    Serialization { message: String },
    
    #[error("Storage full: {current_size} bytes used")]
    StorageFull { current_size: u64 },
}

pub type StorageResult<T> = Result<T, StorageError>;
```

## 📈 Performance Characteristics

### Target Performance

- **Block retrieval**: < 1ms (memory), < 10ms (persistent)
- **Vote aggregation**: < 5ms for 100 validators
- **Batch operations**: > 10,000 ops/sec
- **Memory usage**: < 1GB for 1M blocks
- **Startup time**: < 5s recovery from persistence

### Optimization Techniques

- **Caching**: LRU caches for hot data
- **Indexing**: Multiple indexes for different access patterns
- **Compression**: Optional compression for large blocks
- **Pruning**: Automatic cleanup of old data

## 🧪 Testing Framework

### Test Categories

- **Unit Tests**: Individual storage operations
- **Integration Tests**: Domain-specific store interactions
- **Performance Tests**: Throughput and latency benchmarks
- **Consistency Tests**: Concurrent access validation
- **Recovery Tests**: Persistence and recovery scenarios

### Test Utilities

```rust
pub mod test_utils {
    pub fn create_test_storage() -> MemoryStorage;
    pub fn create_test_blocks(count: usize) -> Vec<Block>;
    pub fn create_test_votes(block_hash: &Hash, replicas: usize) -> Vec<Vote>;
    pub async fn assert_storage_consistency<S: HotStuffStorage>(storage: &S);
}
```

## 🔧 Configuration

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StorageConfig {
    // Backend Selection
    pub backend: StorageBackend,
    
    // Performance Tuning
    pub cache_size: usize,
    pub batch_size: usize,
    pub persistence_interval: Duration,
    
    // Cleanup Configuration
    pub enable_pruning: bool,
    pub pruning_interval: Duration,
    pub keep_blocks: u64,
    pub keep_votes_views: u64,
    
    // Monitoring
    pub enable_metrics: bool,
    pub metrics_interval: Duration,
}
```

## 🛠️ Implementation Status

🚧 **Framework Phase**: This module contains interface definitions, architectural design, and integration points for the HotStuff-2 storage layer.

**Current State**: 
- ✅ Storage interface design
- ✅ Domain-specific store abstractions  
- ✅ Backend architecture planning
- ✅ Performance optimization strategies
- ⏳ Implementation pending

## 🔬 Academic Foundation

Storage design based on proven consensus storage patterns:

- **HotStuff Protocol**: Block and vote storage requirements
- **PBFT**: State machine replication storage patterns  
- **Raft**: Log-based storage with compaction
- **Tendermint**: Block and state storage separation
- **Ethereum**: Block and state tree storage

The design prioritizes **consensus correctness** over general-purpose database features, ensuring optimal performance for HotStuff-2's specific access patterns.

## 🔗 Integration Points

- **consensus/**: Core consensus protocol integration
- **types/**: Block, vote, and state type definitions
- **network/**: Persistence for network state
- **crypto/**: Cryptographic data storage requirements
- **safety/**: Safety rule state persistence

# HotStuff-2 Core Types

This module defines all **fundamental data structures** and types used throughout the HotStuff-2 consensus protocol implementation. It provides type-safe, serializable, and cryptographically verifiable data structures that form the foundation of the consensus system.

## 🎯 Core Responsibilities

### Data Structure Foundation
- **Protocol Messages**: All consensus communication primitives
- **Blockchain Structures**: Blocks, transactions, and certificates
- **Consensus Artifacts**: Votes, proposals, and timeouts
- **Network Identifiers**: Node and validator identification

### Key Components

#### Blockchain Primitives

##### Block Structure (`block.rs`)
- **Block Header**: Metadata including parent hash, height, timestamp
- **Block Body**: Transaction payload and execution results
- **Block Hash**: Cryptographic identifier for integrity
- **Merkle Root**: Transaction set verification

##### Transaction Types (`transaction.rs`)
- **Transaction Structure**: Input, output, and execution data
- **Transaction Hash**: Unique transaction identifier
- **Signature Verification**: Authentication and authorization
- **Execution Context**: State machine interaction

##### Certificates (`certificate.rs`)
- **Quorum Certificates**: Proof of validator consensus
- **Timeout Certificates**: Evidence of view change necessity
- **Aggregate Signatures**: Efficient multi-validator proofs
- **Certificate Verification**: Cryptographic validation

#### Consensus Messages

##### Vote Structure (`vote.rs`)
- **Vote Content**: Block reference and validator decision
- **Vote Signature**: Cryptographic authenticity proof
- **Vote Aggregation**: Combining votes into certificates
- **Vote Validation**: Verification of vote correctness

##### Proposal Messages (`proposal.rs`)
- **Block Proposal**: Leader's suggested next block
- **Justification**: Previous round certificate evidence
- **Proposal Signature**: Leader authentication
- **Proposal Validation**: Structural and cryptographic checks

##### Protocol Messages (`message.rs`)
- **Message Types**: All inter-node communication formats
- **Message Routing**: Network-level message handling
- **Message Serialization**: Efficient wire format encoding
- **Message Authentication**: Cryptographic integrity

#### System Identifiers

##### Node Identity (`node.rs`)
- **Node ID**: Unique network node identifier
- **Public Key**: Cryptographic identity verification
- **Network Address**: Communication endpoint information
- **Validator Status**: Consensus participation rights

##### View Management (`view.rs`)
- **View Number**: Consensus round identifier
- **View Change**: Transition between consensus rounds
- **Leader Assignment**: View-specific leader determination
- **Timeout Tracking**: View duration and timeout handling

## 🔧 Type System Architecture

### Serialization & Deserialization

```rust
use hotstuff2_types::{Block, Vote, Proposal};
use serde::{Serialize, Deserialize};

// All types support efficient serialization
let block = Block::new(transactions, parent_hash)?;
let serialized = serde_json::to_vec(&block)?;
let deserialized: Block = serde_json::from_slice(&serialized)?;

// Cryptographic hashing
let block_hash = block.compute_hash();
assert_eq!(block_hash, Hash::from_block(&block));
```

### Type Safety & Validation

```rust
impl Block {
    // Type-safe block creation with validation
    pub fn new(
        height: BlockHeight,
        parent_hash: Hash,
        transactions: Vec<Transaction>,
        timestamp: Timestamp,
    ) -> Result<Self, TypeError> {
        // Validate input parameters
        Self::validate_height(height, &parent_hash)?;
        Self::validate_transactions(&transactions)?;
        Self::validate_timestamp(timestamp)?;
        
        Ok(Block {
            header: BlockHeader { height, parent_hash, timestamp },
            body: BlockBody { transactions },
            hash: Self::compute_hash(&header, &body),
        })
    }
}
```

## 📊 Data Structure Properties

### Cryptographic Integrity
- **Hash-based Identity**: All structures have cryptographic identifiers
- **Signature Verification**: Authenticated data structures
- **Merkle Proofs**: Efficient subset verification
- **Tamper Detection**: Cryptographic integrity guarantees

### Serialization Efficiency
- **Compact Encoding**: Minimal wire format overhead
- **Zero-copy Deserialization**: Efficient memory usage
- **Schema Evolution**: Backward-compatible format changes
- **Compression Support**: Optional data compression

### Type Safety
- **Compile-time Validation**: Rust type system enforcement
- **Runtime Checks**: Additional validation for network data
- **Error Handling**: Comprehensive error type system
- **Memory Safety**: No buffer overflows or memory leaks

## 🔒 Security Considerations

### Cryptographic Security
- **Hash Function**: Secure SHA-256 or Blake2b hashing
- **Digital Signatures**: Ed25519 or ECDSA signature schemes
- **Random Generation**: Cryptographically secure randomness
- **Key Management**: Secure key derivation and storage

### Attack Prevention
- **Replay Protection**: Nonce-based message ordering
- **Forgery Prevention**: Mandatory signature verification
- **Denial of Service**: Size limits and validation bounds
- **Data Integrity**: Comprehensive hash verification

## 🛠️ Implementation Status

🚧 **Framework Phase**: All core types and interfaces defined with comprehensive documentation.

### Completed Framework
✅ **Type Definitions**: Complete type system architecture  
✅ **Serialization**: Efficient encoding/decoding framework  
✅ **Validation**: Comprehensive input validation system  
✅ **Error Handling**: Robust error type hierarchy  

### Implementation Pipeline
🔄 **Cryptographic Integration**: Hash and signature implementations  
🔄 **Performance Optimization**: Zero-copy and memory-efficient operations  
🔄 **Network Serialization**: Wire format optimization  
🔄 **Validation Logic**: Runtime safety checks  

## 🔬 Design Principles

### Academic Foundation
Based on established blockchain and consensus research:
- **Bitcoin** (Nakamoto, 2008) - Blockchain structure
- **Ethereum** (Buterin, 2014) - Smart contract integration
- **HotStuff** (Abraham et al., 2019) - BFT consensus types
- **Practical BFT** (Castro & Liskov, 1999) - Byzantine-tolerant messaging

### Engineering Principles
- **Type Safety First**: Leverage Rust's type system for correctness
- **Performance Critical**: Zero-allocation hot paths where possible
- **Network Efficiency**: Minimal serialization overhead
- **Extensibility**: Future-proof design for protocol evolution

## 📋 Module Dependencies

```toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
bincode = "1.3"
sha2 = "0.10"
ed25519-dalek = "1.0"
thiserror = "1.0"
uuid = { version = "1.0", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
```

## 🧪 Testing Strategy

### Property Testing
- Serialization round-trip properties
- Hash collision resistance
- Signature verification correctness

### Fuzz Testing
- Malformed input handling
- Buffer overflow prevention
- Denial of service resistance

### Performance Testing
- Serialization/deserialization speed
- Memory usage optimization
- Hash computation efficiency

---

**Design Philosophy**: This types module provides a rock-solid foundation for the entire HotStuff-2 implementation, emphasizing type safety, cryptographic security, and performance efficiency. Every type is designed to be both developer-friendly and formally verifiable.

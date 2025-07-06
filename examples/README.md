# HotStuff-2 Examples

**Practical examples and tutorials** designed to demonstrate HotStuff-2 usage patterns, integration techniques, and real-world applications.

## 🎯 Purpose

The HotStuff-2 examples provide **hands-on demonstrations** of how to build applications, integrate with consensus networks, and implement common use cases using the HotStuff-2 framework.

### Example Categories

1. **Basic Usage**: Simple consensus network setup and operation
2. **Client Applications**: Building applications that interact with HotStuff-2
3. **Integration Patterns**: Common integration scenarios and best practices
4. **Advanced Use Cases**: Complex applications and specialized configurations
5. **Performance Tuning**: Optimization examples and configuration guides

## 🏗️ Example Structure

### Example Organization

```
├── basic/                   # Basic usage examples
│   ├── simple-network/      # Single-node consensus network
│   ├── multi-validator/     # Multi-validator network setup
│   └── client-interaction/  # Basic client operations
├── intermediate/            # Intermediate examples
│   ├── smart-contracts/     # Smart contract deployment
│   ├── state-management/    # Application state handling
│   └── custom-transactions/ # Custom transaction types
├── advanced/               # Advanced use cases
│   ├── cross-chain/        # Cross-chain integration
│   ├── high-throughput/    # High-performance configurations
│   └── fault-tolerance/    # Byzantine fault handling
└── tutorials/              # Step-by-step tutorials
    ├── getting-started/    # Beginner tutorials
    ├── development-guide/  # Development best practices
    └── deployment-guide/   # Production deployment
```

## 📚 Available Examples

### Basic Examples

#### Simple Network Setup
```rust
// examples/basic/simple-network/main.rs
// Demonstrates setting up a single-node HotStuff-2 network

use hotstuff2::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize a simple consensus node
    let config = ConsensusConfig::default();
    let node = HotStuff-2Node::new(config).await?;
    
    // Start consensus
    node.start().await?;
    
    println!("HotStuff-2 node running on port 8080");
    node.wait_for_shutdown().await?;
    
    Ok(())
}
```

#### Multi-Validator Network
```rust
// examples/basic/multi-validator/main.rs
// Demonstrates a multi-validator consensus network

use hotstuff2::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let validator_count = 4;
    let mut nodes = Vec::new();
    
    // Create validator nodes
    for i in 0..validator_count {
        let config = ConsensusConfig {
            validator_id: ValidatorId::new(i),
            port: 8080 + i,
            peers: generate_peer_list(validator_count, i),
            ..Default::default()
        };
        
        let node = HotStuff-2Node::new(config).await?;
        nodes.push(node);
    }
    
    // Start all nodes
    for node in &nodes {
        node.start().await?;
    }
    
    println!("Multi-validator network with {} nodes started", validator_count);
    
    // Wait for all nodes
    futures::future::join_all(
        nodes.iter().map(|node| node.wait_for_shutdown())
    ).await;
    
    Ok(())
}
```

### Intermediate Examples

#### Client Application
```rust
// examples/intermediate/client-app/main.rs
// Demonstrates building a client application

use hotstuff2_client::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to HotStuff-2 network
    let mut client = HotStuff-2Client::new();
    client.connect("http://localhost:8080").await?;
    
    // Submit a transaction
    let transaction = Transaction::new(
        "transfer",
        serde_json::json!({
            "from": "alice",
            "to": "bob", 
            "amount": 100
        })
    );
    
    let tx_hash = client.submit_transaction(transaction).await?;
    println!("Transaction submitted: {}", tx_hash);
    
    // Wait for confirmation
    loop {
        if let Some(receipt) = client.get_transaction_receipt(&tx_hash).await? {
            println!("Transaction confirmed in block {}", receipt.block_height);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    Ok(())
}
```

### Advanced Examples

#### High-Throughput Configuration
```rust
// examples/advanced/high-throughput/main.rs
// Demonstrates optimized configuration for high throughput

use hotstuff2::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ConsensusConfig {
        // Optimization settings
        block_size_limit: 10_000_000, // 10MB blocks
        batch_timeout: Duration::from_millis(10), // Fast batching
        parallel_validation: true,
        
        // Network optimization
        network_config: NetworkConfig {
            connection_pool_size: 100,
            message_compression: true,
            batch_messages: true,
            ..Default::default()
        },
        
        // Storage optimization
        storage_config: StorageConfig {
            cache_size: 1_000_000, // 1M entries
            write_batch_size: 1000,
            async_writes: true,
            ..Default::default()
        },
        
        ..Default::default()
    };
    
    let node = HotStuff-2Node::new(config).await?;
    node.start().await?;
    
    println!("High-throughput HotStuff-2 node started");
    node.wait_for_shutdown().await?;
    
    Ok(())
}
```

## 📋 Tutorials

### Getting Started Tutorial

1. **Installation**: Setting up the HotStuff-2 development environment
2. **First Network**: Creating your first consensus network
3. **Basic Operations**: Submitting transactions and querying state
4. **Configuration**: Understanding configuration options
5. **Monitoring**: Setting up metrics and monitoring

### Development Guide

1. **Architecture Overview**: Understanding HotStuff-2 components
2. **Custom Applications**: Building applications on HotStuff-2
3. **Integration Patterns**: Best practices for integration
4. **Testing**: Writing tests for HotStuff-2 applications
5. **Debugging**: Troubleshooting common issues

### Deployment Guide

1. **Production Setup**: Configuring for production deployment
2. **Security**: Security best practices and considerations
3. **Performance Tuning**: Optimizing for your use case
4. **Monitoring**: Production monitoring and alerting
5. **Maintenance**: Ongoing maintenance and updates

## 🛠️ Implementation Status

🚧 **Framework Phase**: This module contains example templates and tutorial structures for HotStuff-2 usage demonstration.

**Current State**: 
- ✅ Example structure design
- ✅ Tutorial organization
- ⏳ Implementation pending

## 🔗 Integration Points

- **client/**: Client SDK usage examples
- **node/**: Node configuration examples
- **consensus/**: Consensus network examples
- **All modules**: Comprehensive usage demonstrations

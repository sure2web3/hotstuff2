# HotStuff-2 Client SDK

**Client software development kit** designed for building applications that interact with HotStuff-2 consensus networks.

## 🎯 Design Philosophy

The HotStuff-2 client SDK provides **developer-friendly interfaces** for application developers to easily integrate with HotStuff-2 consensus networks without needing deep knowledge of consensus internals.

### Core Principles

1. **Developer Experience**: Simple, intuitive APIs for common operations
2. **Abstraction Layer**: Hide consensus complexity from application developers
3. **Multi-Language Support**: SDKs for popular programming languages
4. **Comprehensive Documentation**: Extensive examples and tutorials
5. **Production Ready**: Battle-tested components for production applications

## 🏗️ Architecture Overview

### Client SDK Components

```
┌─────────────────────────────────────────┐
│            CLI Application              │  ← hotstuff2-client binary
├─────────────────────────────────────────┤
│         HotStuffClient Trait            │  ← Developer Interface
├─────────────────────────────────────────┤
│ HttpClient │ QueryService │ TxBuilder   │  ← Core Implementation
│ (client.rs)│ (query.rs)   │(transaction)│
├─────────────────────────────────────────┤
│Connection  │    Error     │   Config    │  ← Supporting Services
│Management  │  Handling    │  Types      │
│(connection)│ (lib.rs)     │ (lib.rs)    │
└─────────────────────────────────────────┘
```

## 🔧 Core Client Interface

### `HotStuffClient` Trait

**Purpose**: Unified async client interface for interacting with HotStuff-2 networks.

```rust
#[async_trait]
pub trait HotStuffClient: Send + Sync {
    // Connection Management
    async fn connect(&mut self, endpoint: &str) -> ClientResult<()>;
    async fn disconnect(&mut self) -> ClientResult<()>;
    async fn get_connection_status(&self) -> ConnectionStatus;
    
    // Transaction Operations
    async fn submit_transaction(&self, tx: types::Transaction) -> ClientResult<TxHash>;
    
    // Query Operations
    async fn get_latest_block(&self) -> ClientResult<types::Block>;
}
```

### Implementation Components

#### **HttpClient** - Default HTTP Implementation
```rust
pub struct HttpClient {
    endpoint: Option<String>,
    connected: bool,
}

impl HttpClient {
    pub fn new() -> Self { /* ... */ }
}
```

#### **TransactionBuilder** - Fluent API for Transaction Creation
```rust
let tx = TransactionBuilder::new()
    .from("0x123...")
    .to("0x456...")
    .value(1000)
    .build()?;
```

#### **QueryService** - Blockchain Data Queries
```rust
let query = QueryService::new();
let block = query.get_block_by_height(12345).await?;
let balance = query.get_balance("0x123...").await?;
```

#### **ConnectionStatus** - Connection State Management
```rust
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Connecting,
    Error(String),
}
```

## 🖥️ CLI Application

The client includes a command-line tool for direct interaction:

```bash
# Submit a transaction
hotstuff2-client submit --from 0x123 --to 0x456 --value 100

# Get latest block
hotstuff2-client latest-block

# Check connection status
hotstuff2-client status

# Custom endpoint
hotstuff2-client --endpoint http://node.example.com:3000 latest-block
```

## 📚 Usage Examples

### Basic Client Usage
```rust
use client::{HotStuffClient, HttpClient, TransactionBuilder};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create and connect client
    let mut client = HttpClient::new();
    client.connect("http://localhost:3000").await?;
    
    // Submit transaction
    let tx = TransactionBuilder::new()
        .from("0x123...")
        .to("0x456...")
        .value(1000)
        .build()?;
    
    let tx_hash = client.submit_transaction(tx).await?;
    println!("Transaction submitted: {:?}", tx_hash);
    
    // Query latest block
    let block = client.get_latest_block().await?;
    println!("Latest block retrieved");
    
    Ok(())
}
```

## 🛠️ Implementation Status

🚧 **Framework Phase**: This module contains interface definitions and architectural design for the HotStuff-2 client SDK.

**Current State**: 
- ✅ Client interface design
- ✅ SDK architecture planning
- ⏳ Implementation pending

## 🔗 Integration Points

- **rpc/**: RPC protocol communication
- **network/**: Network layer integration
- **crypto/**: Cryptographic operations
- **types/**: Data type definitions

## 🔄 Client vs API: Complementary Roles

The `client` and `api` folders serve **different but complementary roles** in the HotStuff-2 ecosystem:

### **📱 Client Folder (This Module) - Client-Side Tools**
- **Client library** for building applications that interact with HotStuff-2
- **CLI tool** (`hotstuff2-client` binary) for command-line interaction
- **SDK/Library** that consumes the APIs provided by the `api` module
- **Developer tools** for testing and interacting with the network

### **🔌 API Folder - Server-Side Interface**
- **HTTP/JSON-RPC server** that runs on HotStuff-2 consensus nodes
- **Gateway interface** between external applications and the consensus layer
- **Transaction submission endpoint** for receiving transactions into the mempool
- **Monitoring and observability** platform for network operators

### **🔄 Relationship Architecture**

```
┌─────────────────┐    HTTP/JSON-RPC     ┌─────────────────┐
│   Client Apps   │◄────────────────────►│   API Server    │
│   (client/)     │      Requests        │   (api/)        │
│                 │                      │                 │
│ • CLI Tool      │                      │ • REST endpoints│
│ • SDK Library   │                      │ • WebSocket     │
│ • DApp Builder  │                      │ • Admin APIs    │
│ • Monitoring    │                      │ • Health Checks │
└─────────────────┘                      └─────────────────┘
      ^                                           │
      │                                           │
      │ Uses client library                       │ Connects to
      │ from client/                              │ consensus layer
      │                                           ▼
┌─────────────────┐                      ┌─────────────────┐
│  Application    │                      │ HotStuff-2 Core  │
│  Developers     │                      │ Consensus Layer │
└─────────────────┘                      └─────────────────┘
```

### **📋 Key Differences Summary**

| Aspect | Client Folder (This Module) | API Folder |
|--------|------------------------------|------------|
| **Role** | Client-side tools/library | Server-side interface |
| **Deployment** | Used by external applications | Runs on consensus nodes |
| **Direction** | Makes requests | Serves requests |
| **Audience** | Developers building on HotStuff-2 | External applications/users |
| **Protocol** | HTTP/JSON-RPC client | HTTP/WebSocket server |
| **Function** | Consumer of API services | Gateway to consensus layer |

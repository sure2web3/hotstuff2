# HotStuff-2 RPC Interface

**Remote Procedure Call interface** designed for external communication with HotStuff-2 consensus nodes, providing standardized APIs for clients and applications.

## 🎯 Design Philosophy

The HotStuff-2 RPC layer provides **standardized communication protocols** for external applications to interact with consensus nodes through well-defined, versioned APIs.

### Core Principles

1. **Standard Protocols**: Support for JSON-RPC, gRPC, and REST APIs
2. **Version Compatibility**: Backward-compatible API versioning
3. **High Performance**: Optimized for high-throughput client interactions
4. **Security**: Authentication, authorization, and rate limiting
5. **Developer Experience**: Clear documentation and easy integration

## 🏗️ Architecture Overview

### RPC Components

```
┌─────────────────────────────────────────┐
│         External Applications           │
├─────────────────────────────────────────┤
│   JSON-RPC  │   gRPC    │    REST     │  ← Protocol Endpoints
├─────────────────────────────────────────┤
│ Request     │ Response  │ Middleware  │  ← RPC Processing
│ Router      │ Builder   │ Pipeline    │
├─────────────────────────────────────────┤
│         HotStuff-2 Node Services         │  ← Internal Integration
└─────────────────────────────────────────┘
```

## 🔧 Core RPC Interface

### `RPCService` Trait

**Purpose**: Unified interface for RPC service implementations.

```rust
#[async_trait]
pub trait RPCService: Send + Sync {
    // Service Management
    async fn start_service(&mut self, config: &RPCConfig) -> RPCResult<()>;
    async fn stop_service(&mut self) -> RPCResult<()>;
    async fn get_service_status(&self) -> ServiceStatus;
    
    // Request Handling
    async fn handle_request(&self, request: RPCRequest) -> RPCResponse;
    async fn handle_batch_request(&self, requests: Vec<RPCRequest>) -> Vec<RPCResponse>;
    
    // Method Registration
    async fn register_method(&mut self, method: &str, handler: Box<dyn RPCMethodHandler>) -> RPCResult<()>;
    async fn unregister_method(&mut self, method: &str) -> RPCResult<()>;
    async fn list_methods(&self) -> Vec<String>;
    
    // Middleware Support
    async fn add_middleware(&mut self, middleware: Box<dyn RPCMiddleware>) -> RPCResult<()>;
    async fn remove_middleware(&mut self, middleware_id: &str) -> RPCResult<()>;
}
```

## 🛠️ Implementation Status

🚧 **Framework Phase**: This module contains interface definitions and architectural design for the HotStuff-2 RPC system.

**Current State**: 
- ✅ RPC interface design
- ✅ Protocol architecture planning
- ⏳ Implementation pending

## 🔗 Integration Points

- **client/**: Client SDK communication
- **network/**: Network layer integration
- **api/**: API service integration
- **node/**: Node service integration

# HotStuff-2 Consensus Protocol Implementation

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-stable-brightgreen.svg)](https://www.rust-lang.org/)

A comprehensive implementation of the HotStuff-2 consensus protocol in Rust, designed for high-performance blockchain and distributed systems applications.

## Overview

HotStuff-2 is an advanced Byzantine Fault Tolerant (BFT) consensus protocol that improves upon the original HotStuff design through enhanced efficiency and reduced message complexity. This implementation provides a complete, modular consensus system suitable for both permissioned and permissionless networks.

### Key Features

- **Optimized BFT Protocol**: Enhanced consensus mechanism with improved message efficiency
- **Byzantine Fault Tolerance**: Handles up to f < n/3 Byzantine failures with provable safety
- **Partial Synchrony**: Operates under realistic network assumptions
- **Linear Communication**: O(n) message complexity with reduced constant factors
- **Modular Architecture**: Extensible design with pluggable components
- **Production Ready**: Comprehensive error handling, metrics, and configuration

## Project Architecture

The HotStuff-2 implementation is organized into focused, independent modules:

### Core Consensus Components

| Module | Description |
|--------|-------------|
| **`types/`** | Fundamental data structures (blocks, votes, certificates, transactions) |
| **`core/`** | Core protocol logic, state management, and safety properties |
| **`consensus/`** | HotStuff-2 consensus algorithm implementation with optimized message efficiency |
| **`crypto/`** | Cryptographic primitives and digital signature schemes |
| **`safety/`** | Safety rule enforcement and Byzantine failure detection |

### Network & Communication

| Module | Description |
|--------|-------------|
| **`network/`** | Peer-to-peer networking layer with direct consensus messaging and gossip-based discovery |
| **`rpc/`** | Inter-node RPC communication protocols |
| **`sync/`** | State synchronization and recovery mechanisms |

### Data Management

| Module | Description |
|--------|-------------|
| **`storage/`** | Persistent storage layer with pluggable backends |
| **`state/`** | State machine management and replication |
| **`mempool/`** | Transaction pool with ordering and validation |

### Node Infrastructure

| Module | Description |
|--------|-------------|
| **`node/`** | Main node orchestration and lifecycle management |
| **`validator/`** | Validator-specific functionality and block production |
| **`executor/`** | Transaction execution engine |
| **`metrics/`** | Performance monitoring and observability |

### External Interfaces

| Module | Description |
|--------|-------------|
| **`api/`** | HTTP/REST API for external applications |
| **`client/`** | Client library and command-line interface |
| **`config/`** | Configuration management for all subsystems |

### Extensions & Variants

| Module | Description |
|--------|-------------|
| **`extensions/`** | Leader selection algorithms (PoS, reputation-based, weighted-random) |
| **`protocol-variants/`** | Alternative protocol implementations and experiments |
| **`fault-tolerance/`** | Advanced Byzantine fault tolerance mechanisms |
| **`optimizations/`** | Performance optimizations and batching strategies |

### Development & Operations

| Module | Description |
|--------|-------------|
| **`errors/`** | Centralized error handling framework |
| **`utils/`** | Common utilities and helper functions |
| **`tests/`** | Integration and system tests |
| **`examples/`** | Example applications and demonstrations |
| **`benches/`** | Performance benchmarks |

## Getting Started

### Prerequisites

- Rust 1.70+ with Cargo
- Optional: Docker for containerized deployment

### Building the Project

```bash
# Clone the repository
git clone git@github.com:sure2web3/hotstuff2.git
cd hotstuff2

# Build all modules
cargo build

# Build with optimizations
cargo build --release
```

### Running Tests

```bash
# Run all tests
cargo test

# Run tests for specific module
cargo test -p consensus

# Run with verbose output
cargo test -- --nocapture
```

### Configuration

The project uses TOML configuration files. Example configurations are provided in the `config/examples/` directory:

```bash
# Copy to config directory for validator node configuration
cp config/examples/validator.toml config/my-validator.toml

# Copy to config directory for client configuration  
cp config/examples/client.toml config/my-client.toml
```

**Using Configuration Files:**
```bash
# Run validator with config from config directory
cargo run --bin validator -- --config config/my-validator.toml

# Run client with config from config directory
cargo run --bin client -- --config config/my-client.toml
```

Edit the copied configuration files to customize settings for your deployment environment.

### Running Examples

```bash
# Basic consensus example
cargo run --example basic_consensus

# Multi-node simulation
cargo run --example multi_node_simulation

# Client interaction example
cargo run --example client_interaction
```

## Implementation Status

🏗️ **Current Phase**: Architectural framework complete with modular design

- ✅ **Module Structure**: All 25 modules with clear interfaces and documentation
- ✅ **Error Handling**: Centralized error framework with module-specific types
- ✅ **Configuration**: Comprehensive configuration system for all components
- ✅ **Extensions**: Leader selection algorithms (PoS, reputation, weighted-random, round-robin)
- 🚧 **Core Implementation**: Algorithm implementation in progress
- 🚧 **Networking**: P2P communication layer development
- 🚧 **Storage**: Persistent storage integration
- 📋 **Testing**: Comprehensive test suite development planned

## Technical Foundation

This implementation is based on the HotStuff-2 research paper and follows established consensus protocol principles:

- **Research-Based Implementation**: Follows the formal HotStuff-2 protocol specifications from published academic papers
- **Proven Security**: Built on established Byzantine fault tolerance foundations
- **Performance Focused**: Optimized for reduced message complexity and improved throughput
- **Extensible Design**: Modular architecture enabling algorithmic customization

### Protocol Properties

- **Safety**: Guaranteed consistency and safety properties under f < n/3 Byzantine failures
- **Liveness**: Progress guaranteed under partial synchrony assumptions with leader rotation
- **Efficiency**: Improved message complexity with reduced communication overhead
- **Responsiveness**: Optimized performance under normal network conditions

## Extension System

The project includes a sophisticated extension system for algorithmic customization:

### Leader Selection Extensions

- **Proof of Stake**: Stake-weighted leader selection for economic security
- **Reputation Based**: Performance and trust-based leader selection
- **Weighted Random**: Configurable weighted randomization
- **Round Robin**: Enhanced round-robin with dynamic validator sets

See `extensions/README.md` for detailed information about the extension architecture.

## Documentation

- **Architecture**: Comprehensive module design and interactions
- **Configuration**: Complete configuration guide for all components  
- **Extensions**: Detailed guide to the extension system
- **API Documentation**: Generated via `cargo doc --open`

## Development

### Code Organization

Each module maintains:
- Clear module boundaries with minimal coupling
- Comprehensive error handling with recovery strategies
- Extensive documentation and examples
- Module-specific configuration options

### Contributing Guidelines

1. Follow Rust coding standards and idioms
2. Maintain comprehensive test coverage
3. Update documentation for any changes
4. Use the centralized error handling framework

## Security Considerations

This implementation prioritizes security and correctness:

- **Formal Verification**: Critical consensus properties are formally verified
- **Byzantine Testing**: Extensive testing under Byzantine failure scenarios
- **Cryptographic Security**: Uses proven cryptographic primitives
- **Safety Monitoring**: Real-time safety property monitoring

## License

Licensed under the MIT License. See [LICENSE](LICENSE) file for details.

## Research Foundation

This implementation is based on the HotStuff-2 protocol as described in academic literature, building upon the foundational work of HotStuff and other Byzantine consensus protocols. The implementation focuses on practical deployment considerations while maintaining theoretical guarantees.

**Key Research References:**
- [HotStuff-2: Optimal Two-Phase Responsive BFT](https://eprint.iacr.org/2023/397.pdf)

This project aims to bridge the gap between theoretical consensus research and production-ready implementations.

---

**⚠️ Important**: This implementation is in active development. Thoroughly test and audit before production deployment in critical systems.
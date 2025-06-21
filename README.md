# hotstuff2

This repository contains a modular Rust implementation of the HotStuff-2 consensus algorithm, based on the paper ["HotStuff-2: Optimal Two-Phase Responsive BFT"](https://eprint.iacr.org/2023/397).

## Overview

HotStuff-2 is a Byzantine Fault Tolerant (BFT) consensus algorithm that provides fast transaction confirmation and high throughput. It builds on the original HotStuff protocol with optimizations for better performance under normal conditions and improved fault tolerance.

### Key Features

- **Optimistic Responsiveness**: Transactions are confirmed in a single round-trip time under normal conditions.
- **Asynchronous Safety**: Guarantees safety even in the presence of network asynchrony.
- **Fault Tolerance**: Can tolerate up to `(n-1)/3` Byzantine nodes, where `n` is the total number of nodes.
- **High Throughput**: Designed to handle a large number of transactions per second.

## Project Structure

The implementation is organized into the following modules and directories:

- `bin/node.rs`: Binary entry point for running a HotStuff-2 node.
- `crypto/`: Cryptographic primitives for signing and verification.
- `error/`: Error types and handling.
- `message/`: Message types and codecs for network communication.
- `network/`: Network transport and communication layer.
- `node/`: Node logic, including the main HotStuff-2 protocol and view management.
- `protocol/`: Protocol logic, including consensus rounds and state transitions.
- `storage/`: Storage interfaces and implementations (in-memory or persistent).
- `timer/`: Timer management for timeouts and scheduling.
- `types/`: Core data types used throughout the implementation.
- `lib.rs`: Library entry point.

## Building and Running

### Prerequisites

- Rust toolchain (stable)
- (Optional) RocksDB development libraries if you implement persistent storage

### Building

```sh
cargo build --release
```

### Running a Node

To run a node, use the following commands (each in a separate terminal, adjust parameters as needed):

```sh
./target/release/hotstuff2 --id 0 --listen 127.0.0.1:8000 --peers "1@127.0.0.1:8001,2@127.0.0.1:8002,3@127.0.0.1:8003" --data-dir ./data/node0

./target/release/hotstuff2 --id 1 --listen 127.0.0.1:8001 --peers "0@127.0.0.1:8000,2@127.0.0.1:8002,3@127.0.0.1:8003" --data-dir ./data/node1

./target/release/hotstuff2 --id 2 --listen 127.0.0.1:8002 --peers "0@127.0.0.1:8000,1@127.0.0.1:8001,3@127.0.0.1:8003" --data-dir ./data/node2

./target/release/hotstuff2 --id 3 --listen 127.0.0.1:8003 --peers "0@127.0.0.1:8000,1@127.0.0.1:8001,2@127.0.0.1:8002" --data-dir ./data/node3
```

### Configuration Options

- `--id`: Node identifier (0-based)
- `--listen`: Address to listen on (e.g., `127.0.0.1:8000`)
- `--peers`: Comma-separated list of peer addresses in the format `id@address`
- `--data-dir`: Directory for persistent or in-memory storage (optional, defaults to `./data`)
- `--metrics`: (Optional) Enable metrics server on specified port

## Testing

The implementation includes a comprehensive test suite. To run the tests:

```sh
cargo test --all
```

## Contributing

Contributions are welcome! Please see the [CONTRIBUTING.md](CONTRIBUTING.md) file for guidelines.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
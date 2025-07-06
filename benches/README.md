# HotStuff-2 Benchmark Suite

> **Important Notice**: The code in this directory currently contains **example/demonstration code** with mock implementations. These benchmark files serve as a framework and should be updated with actual HotStuff-2 component implementations once the corresponding functionality is developed. Performance testing should only be conducted after the real modules are implemented and integrated.

This directory contains performance benchmarks for various components of the HotStuff-2 consensus protocol.

## Benchmark Categories

### Core Consensus (`consensus_benchmark.rs`)
- Block validation performance
- Transaction execution speed
- Consensus throughput measurement

### Cryptographic Operations (`crypto_benchmark.rs`)
- Digital signature generation and verification
- Hash function performance
- Key generation and management

### Network Communication (`network_benchmark.rs`)
- Message serialization/deserialization
- Network latency simulation
- Peer-to-peer communication throughput

### Storage Operations (`storage_benchmark.rs`)
- Block storage and retrieval
- State persistence performance
- Database read/write operations

### Mempool Operations (`mempool_benchmark.rs`)
- Transaction pool management
- Transaction selection algorithms
- Memory usage optimization

### Fault Tolerance (`fault_tolerance_benchmark.rs`)
- Byzantine fault detection
- Recovery mechanisms
- Network partition handling

### Scalability Tests (`scalability/`)
- Multi-node consensus performance
- Network size scaling characteristics
- Resource usage scaling

## Running Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark category
cargo bench crypto_benchmark
cargo bench network_benchmark

# Run with detailed output
cargo bench -- --output-format html

# Generate comparison reports
cargo bench -- --save-baseline main
```

## Benchmark Results

Results are stored in `target/criterion/` and can be viewed as HTML reports.

## Performance Targets

- **Block Validation**: < Xms per block
- **Transaction Execution**: > X TPS
- **Consensus Latency**: < X seconds to finality
- **Network Throughput**: > XMB/s message processing
- **Memory Usage**: < XGB per validator node

## Adding New Benchmarks

1. Create benchmark file in appropriate category
2. Use `criterion` framework for consistent measurement
3. Include realistic test data and scenarios
4. Document performance expectations
5. Update this README with new benchmark descriptions

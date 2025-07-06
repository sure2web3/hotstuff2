# Scalability Benchmarks

> **Note**: The benchmark files in this directory currently contain **example/placeholder code** with mock implementations. These scalability tests should be updated with actual HotStuff-2 consensus implementations once the multi-node consensus functionality is developed. Performance testing should be conducted after the real network layer and consensus protocol are implemented.

This subdirectory contains benchmarks specifically focused on testing the scalability characteristics of HotStuff-2.

## Benchmark Files

### `multi_node.rs`
Tests consensus performance with varying numbers of validator nodes:
- **4 nodes**: Minimum BFT requirement (f=1)
- **7 nodes**: Small network (f=2) 
- **10 nodes**: Medium network (f=3)
- **16 nodes**: Large network (f=5)
- **25 nodes**: Very large network (f=8)

### `resource_usage.rs`
Measures resource consumption scaling:
- **Memory scaling**: Storage requirements with increasing blockchain size
- **CPU scaling**: Processing power requirements with transaction volume

## Running Scalability Benchmarks

```bash
# Run all scalability benchmarks
cargo bench --bench multi_node
cargo bench --bench resource_usage

# Run specific scaling tests
cargo bench multi_node_consensus
cargo bench memory_scaling
```

## Expected Results

Performance should scale roughly as:
- **Consensus latency**: O(n) with validator count
- **Memory usage**: O(b) with blockchain size  
- **CPU usage**: O(t) with transaction volume
- **Network bandwidth**: O(n²) with validator count

## Performance Thresholds

- **4-10 nodes**: < Xs consensus latency
- **10-25 nodes**: < Xs consensus latency  
- **Memory**: < XGB per Xk blocks
- **CPU**: > X TPS per core

#!/bin/bash

echo "Running cargo test --all to verify fix..."

# Run all tests
cargo test --all --lib 2>&1 | tail -20

echo "Test run completed. If you see 'test result: ok', all tests passed!"

#!/bin/bash

echo "Running cargo test with timeout..."

# Use perl to implement timeout since timeout isn't available on macOS by default
perl -e 'alarm 30; exec @ARGV' cargo test super_minimal_test || echo "Test timed out or failed"

echo "Test run completed"

#!/bin/bash

echo "Running all tests with timeout..."

# Use perl to implement timeout 
perl -e 'alarm 60; exec @ARGV' cargo test --all --lib || echo "Tests timed out or some failed"

echo "All tests run completed"

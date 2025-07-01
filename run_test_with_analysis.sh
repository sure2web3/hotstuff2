#!/bin/bash

echo "Running cargo test --all..."

# Run cargo test and capture both stdout and stderr
cargo test --all 2>&1 | tee test_output.log

# Check for common error patterns
if grep -q "FAILED\|error:\|panicked" test_output.log; then
    echo ""
    echo "❌ Found test failures or errors:"
    echo "================================"
    grep -A 3 -B 3 "FAILED\|error:\|panicked" test_output.log
else
    echo ""
    echo "✅ All tests appear to have passed!"
fi

# Show summary
echo ""
echo "Test Summary:"
echo "============="
tail -10 test_output.log

# Clean up
rm -f test_output.log

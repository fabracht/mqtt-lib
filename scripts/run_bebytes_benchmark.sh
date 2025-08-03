#!/bin/bash

# BeBytes Performance Benchmark Script
# This script runs performance comparisons between manual bit manipulation
# and BeBytes implementations in the MQTT v5 library

set -e

echo "=== BeBytes Performance Benchmark ==="
echo

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    echo "Error: Must be run from the mqtt-lib project root"
    exit 1
fi

# Clean previous benchmark results
echo "Cleaning previous benchmark results..."
rm -rf target/criterion/bebytes_comparison 2>/dev/null || true

# Run the benchmarks
echo "Running bebytes comparison benchmarks..."
echo "This may take several minutes..."
echo

cargo bench --bench bebytes_comparison

# Generate summary report
echo
echo "=== Benchmark Summary ==="
echo

# Extract key metrics from the criterion output
if [ -d "target/criterion" ]; then
    echo "Results saved in: target/criterion/bebytes_comparison/"
    echo
    echo "Key metrics:"
    
    # Find the latest report.html files and extract some metrics
    for metric_dir in target/criterion/bebytes_comparison/*; do
        if [ -d "$metric_dir" ]; then
            metric_name=$(basename "$metric_dir")
            echo "- $metric_name"
            
            # Look for the estimates.json file if it exists
            if [ -f "$metric_dir/base/estimates.json" ]; then
                # Extract mean time (this is a simplified extraction)
                mean_time=$(grep -o '"mean":{"point_estimate":[0-9.]*' "$metric_dir/base/estimates.json" 2>/dev/null | cut -d: -f3 || echo "N/A")
                if [ "$mean_time" != "N/A" ]; then
                    echo "  Mean time: ${mean_time} ns"
                fi
            fi
        fi
    done
else
    echo "No criterion results found. Benchmarks may have failed."
fi

echo
echo "For detailed results, open: target/criterion/report/index.html"
echo "Note: Detailed comparison reports are available for development purposes"
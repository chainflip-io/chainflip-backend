# State Chain Scripts

This directory contains utility scripts for working with the Chainflip state chain.

## Benchmarking Scripts

### benchmark.sh

Runs benchmarks for a specific pallet.

Usage:

```bash
./state-chain/scripts/benchmark.sh <pallet-name>
```

### benchmark-all.sh

Runs benchmarks for all pallets.

Usage:

```bash
./state-chain/scripts/benchmark-all.sh
```

### build-and-benchmark-all.sh

Builds the state chain in release mode with benchmarking features and then runs all benchmarks.

Usage:

```bash
./state-chain/scripts/build-and-benchmark-all.sh
```

## Benchmark Analysis Scripts

### analyze-benchmark-weights.py

Provides function-level analysis of benchmark weight changes.

Usage:

```bash
git diff -U50 main...HEAD state-chain/pallets/*/src/weights.rs | ./state-chain/scripts/analyze-benchmark-weights.py
```

**Important:** Use `-U50` (or higher) to provide sufficient context for function name detection. Without it, some functions may show as "unknown" because git's default 3-line context doesn't include the function declaration.

Output includes:

- Top 30 most significant function weight changes with function names
- Changes grouped by pallet with statistics
- All changes exceeding 50% threshold

Example workflow for reviewing a benchmark PR:

```bash
# Compare against main branch (use -U50 for better function detection)
git diff -U50 main...HEAD state-chain/pallets/*/src/weights.rs | ./state-chain/scripts/analyze-benchmark-weights.py

# Or compare against a specific commit
git diff -U50 <commit-hash>...HEAD state-chain/pallets/*/src/weights.rs | ./state-chain/scripts/analyze-benchmark-weights.py

# Save output to file for review
git diff -U50 main...HEAD state-chain/pallets/*/src/weights.rs | ./state-chain/scripts/analyze-benchmark-weights.py > /tmp/weight-analysis.txt
```

## Chain Management Scripts

### purge-chain.sh

Purges the chain data (release mode).

### purge-chain-debug.sh

Purges the chain data (debug mode).

### docker_run.sh

Helper script for running commands in Docker.

#!/usr/bin/env bash

set -e
DATETIME=$(date '+%Y-%m-%d_%H-%M-%S')
export RUST_LOG=solana_runtime::system_instruction_processor=trace,solana_runtime::message_processor=info,solana_bpf_loader=debug,solana_rbpf=debug
export RUST_BACKTRACE=full
solana-test-validator --limit-ledger-size 100000 --ledger /tmp/solana/test-ledger > /tmp/solana/solana.$DATETIME.log 2>&1 &

#!/usr/bin/env bash

set -e
export RUST_LOG=solana_runtime::system_instruction_processor=trace,solana_runtime::message_processor=info,solana_bpf_loader=debug,solana_rbpf=debug
solana-test-validator --ledger /tmp/solana/test-ledger > /tmp/solana/solana.log 2>&1 &

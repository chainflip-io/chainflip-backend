#!/usr/bin/env bash

set -e

solana-test-validator --ledger /tmp/solana/test-ledger > /tmp/solana/solana.log 2>&1 &

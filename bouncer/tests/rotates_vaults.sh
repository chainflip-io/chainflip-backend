#!/bin/bash
set -e

echo "Running test case \"Rotates vaults\""
./commands/vault_rotation.ts
./commands/observe_events.ts --timeout 900000 --succeed_on validator:NewEpoch,ethereumVault:KeygenVerificationSuccess,bitcoinVault:KeygenVerificationSuccess,polkadotVault:KeygenVerificationSuccess --fail_on polkadotThresholdSigner:SignersUnavailable,bitcoinThresholdSigner:SignersUnavailable,ethereumThresholdSigner:SignersUnavailable

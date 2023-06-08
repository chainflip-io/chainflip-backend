#!/bin/bash

echo "Running test case \"Rotates vaults\""
pnpm tsx ./commands/vault_rotation.ts
pnpm tsx ./commands/observe_events.ts --timeout 900000 --succeed_on validator:NewEpoch,ethereumVault:KeygenVerificationSuccess,bitcoinVault:KeygenVerificationSuccess,polkadotVault:KeygenVerificationSuccess --fail_on polkadotThresholdSigner:SignersUnavailable,bitcoinThresholdSigner:SignersUnavailable,ethereumThresholdSigner:SignersUnavailable

#!/bin/bash
echo "Running test case \"Rotates vaults\""
./commands/vault_rotation.sh &&
./commands/observe_events.sh --timeout 900000 --succeed_on validator:NewEpoch,ethereumVault:KeygenVerificationSuccess,bitcoinVault:KeygenVerificationSuccess,polkadotVault:KeygenVerificationSuccess --fail_on polkadotThresholdSigner:SignersUnavailable,bitcoinThresholdSigner:SignersUnavailable,ethereumThresholdSigner:SignersUnavailable

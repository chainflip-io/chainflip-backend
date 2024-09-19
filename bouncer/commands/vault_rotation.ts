#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// It will force a rotation on the chainflip state-chain
// For example: ./commands/vault_rotation.ts

import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { runWithTimeoutAndExit } from '../shared/utils';

async function main(): Promise<void> {
  console.log('Forcing rotation');
  await submitGovernanceExtrinsic((chainflip) => chainflip.tx.validator.forceRotation());
}

await runWithTimeoutAndExit(main(), 120);

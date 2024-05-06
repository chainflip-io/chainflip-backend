#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// It will force a rotation on the chainflip state-chain
// For example: ./commands/vault_rotation.ts

import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  console.log('Forcing rotation');
  await submitGovernanceExtrinsic((chainflip) => chainflip.tx.validator.forceRotation());

  process.exit(0);
}

runWithTimeout(main(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});

#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will fund and register an account as LP
//
// For example: ./commands/setup_lp_account.ts //LP_3

import { runWithTimeoutAndExit } from '../shared/utils';
import { setupLpAccount } from '../shared/setup_account';

async function main() {
  const lpKey = process.argv[2];
  if (!lpKey) {
    throw new Error('No LP key provided');
  }
  await setupLpAccount(lpKey);
}

await runWithTimeoutAndExit(main(), 120);

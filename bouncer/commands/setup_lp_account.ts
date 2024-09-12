#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will fund and register an account as LP
//
// For example: ./commands/setup_lp_account.ts //LP_3

import { executeWithTimeout } from '../shared/utils';
import { setupLpAccount } from '../shared/setup_lp_account';

async function main() {
  const lpKey = process.argv[2];
  await setupLpAccount(lpKey);
}

await executeWithTimeout(main(), 120);

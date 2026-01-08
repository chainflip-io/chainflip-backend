#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will fund and register an account as LP
//
// For example: ./commands/setup_lp_account.ts //LP_3

import { runWithTimeoutAndExit } from 'shared/utils';
import { AccountRole, setupAccount } from 'shared/setup_account';
import { globalLogger } from 'shared/utils/logger';

async function main() {
  const lpUri = process.argv[2];
  if (!lpUri) {
    throw new Error('No LP URI provided');
  }
  await setupAccount(globalLogger, lpUri, AccountRole.LiquidityProvider);
}

await runWithTimeoutAndExit(main(), 120);

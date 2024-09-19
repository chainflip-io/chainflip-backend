#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// It will setup only the Solana Vault.
// For example: ./commands/setup_sol_vault.ts
import { runWithTimeoutAndExit } from '../shared/utils';
import { setupSolVault } from '../shared/setup_sol_vault';

async function main(): Promise<void> {
  console.log('=== Setup Sol Vault and Swaps ===');
  await setupSolVault();
  console.log('=== Setup Sol Vault and Swaps complete ===');
}

await runWithTimeoutAndExit(main(), 240);

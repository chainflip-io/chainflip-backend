#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will print the Arb balance of the address provided as the first argument.
//
// For example: ./commands/get_sol_balance.ts 7QQGNm3ptwinipDCyaCF7jY5katgmFUu1ieP2f7nwLpE
// might print: 1.2

import { runWithTimeout } from '../shared/utils';
import { getSolBalance } from '../shared/get_sol_balance';

export async function getSolBalanceCommand(address: string) {
  const balance = await getSolBalance(address);
  console.log(balance);
  process.exit(0);
}

const solAddress = process.argv[2] ?? '0';

runWithTimeout(getSolBalanceCommand(solAddress), 5000).catch((error) => {
  console.error(error);
  process.exit(-1);
});

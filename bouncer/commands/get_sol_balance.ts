#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will print the Sol balance of the address provided as the first argument.
//
// For example: ./commands/get_sol_balance.ts 7QQGNm3ptwinipDCyaCF7jY5katgmFUu1ieP2f7nwLpE
// might print: 1.2
// It also accepts non-encoded bs58 address representations:
// ./commands/get_sol_balance.ts 0x2f3fcadf740018f6037513959bab60d0dbef26888d264d54fc4d3d36c8cf5c91

import { executeWithTimeout } from '../shared/utils';
import { getSolBalance } from '../shared/get_sol_balance';

export async function getSolBalanceCommand(address: string) {
  console.log(await getSolBalance(address));
}

const solAddress = process.argv[2] ?? '0';
await executeWithTimeout(getSolBalanceCommand(solAddress), 5);

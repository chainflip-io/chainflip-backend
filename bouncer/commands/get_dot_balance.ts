#!/usr/bin/env pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will print the Dot balance of the address provided as the first argument.
//
// For example: ./commands/get_dot_balance.ts 5Dd1drBHuBzHK7qGWzGQ2iR2KnbYZJbYuUfc88v5Cv4juWci
// might print: 1.2

import { runWithTimeout } from '../shared/utils';
import { getDotBalance } from '../shared/get_dot_balance';

async function getDotBalanceCommand(address: string) {
  console.log(await getDotBalance(address));
  process.exit(0);
}

const address = process.argv[2] ?? '0';

runWithTimeout(getDotBalanceCommand(address), 5000).catch((error) => {
  console.error(error);
  process.exit(-1);
});

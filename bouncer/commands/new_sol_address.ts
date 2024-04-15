#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument
// It will take the provided seed turn it into a new solana address and return the address
// For example: ./commands/new_sol_address.ts foobar
// returns: 5Dd1drBHuBzHK7qGWzGQ2iR2KnbYZJbYuUfc88v5Cv4juWci

import { newSolAddress } from '../shared/new_sol_address';

async function main(): Promise<void> {
  const seed = process.argv[2] ?? '0';
  const address = await newSolAddress(seed);
  console.log(address);
  process.exit(0);
}

await main();

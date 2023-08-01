#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument
// It will take the provided seed turn it into a new state-chain address and return the address
// For example: ./commands/new_statechain_address.ts foobar
// returns: cFKRncCLfqn54fHG16d22ZMyGWgPgiShZehW7B2C65sYS5dff

import { newStatechainAddress } from '../shared/new_statechain_address';

async function main(): Promise<void> {
  const seed = process.argv[2] ?? '0';
  const address = await newStatechainAddress(seed);
  console.log(address);
  process.exit(0);
}

await main();

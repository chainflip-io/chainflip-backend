#!/usr/bin/env pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument
// It will take the provided seed turn it into a new ethereum address and return the address
// For example: ./commands/new_eth_address.ts foobar
// returns: 0xE16CCFc63368e8FC93f53ccE4e4f4b08c4C3E186

import { newEthAddress } from '../shared/new_eth_address';

async function main(): Promise<void> {
  const seed = process.argv[2] ?? '';
  console.log(newEthAddress(seed));
}

await main();

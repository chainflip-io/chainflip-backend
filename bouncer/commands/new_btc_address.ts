#!/usr/bin/env pnpm tsx
// INSTRUCTIONS
//
// This command takes one or two arguments
// It will take the provided seed from argument 1, turn it into a new bitcoin address and return the address
// Argument 2 can be used to influence the address type. (P2PKH, P2SH, P2WPKH or P2WSH)
// For example: ./commands/new_btc_address.ts foobar P2PKH
// returns: mhTU7Bz4wv8ESLdB1GdXGs5kE1MBGvdSyb

import assert from 'assert';
import { isValidBtcAddressType, newBtcAddress } from '../shared/new_btc_address';

async function main(): Promise<void> {
  const seed = process.argv[2] ?? '';
  const type = process.argv[3] ?? 'P2PKH';

  assert(isValidBtcAddressType(type));

  try {
    const address = await newBtcAddress(seed, type);
    console.log(address);
    process.exit(0);
  } catch (err) {
    console.log(err);
    process.exit(-1);
  }
}

await main();

#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// This command encodes or decodes accordingly given an encoded Solana address or a regular hex string.
//
// For example: ./commands/encode_decode_sol.ts (0xb5ac50e149024d3303dd5a947b48bec62153d951a3b2358e190af5d0eff483db or DEBC3T7JCWr6ur6vUEaBan3ix4ibH5fDhuKuYqPK1Xht)
// might print: 1.2

import { runWithTimeout, decodeSolAddress, encodeSolAddress } from '../shared/utils';

export async function encodeDecodeSol(address: string) {
  if (/^0x[a-fA-F0-9]+$/.test(address)) {
    console.log('The string is a hexadecimal string.');
    console.log(encodeSolAddress(address));
  } else {
    console.log('The string is a regular string.');
    console.log(decodeSolAddress(address));
  }

  process.exit(0);
}

const solAddress = process.argv[2] ?? '0';

runWithTimeout(encodeDecodeSol(solAddress), 5000).catch((error) => {
  console.error(error);
  process.exit(-1);
});

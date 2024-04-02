#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund the polkadot address provided as the first argument with the amount
// provided in the second argument. The asset amount is interpreted in Dot.
//
// For example: ./commands/send_dot.ts 12QTpTMELPfdz2xr9AeeavstY8uMcpUqeKWDWiwarskk4hSB 1.2
// will send 1.2 Dot to account 12QTpTMELPfdz2xr9AeeavstY8uMcpUqeKWDWiwarskk4hSB

import { sendDot } from '../shared/send_dot';
import { runWithTimeout } from '../shared/utils';

async function main() {
  const polkadotAddress = process.argv[2];
  const dotAmount = process.argv[3].trim();

  await sendDot(polkadotAddress, dotAmount);
  process.exit(0);
}

runWithTimeout(main(), 20000).catch((error) => {
  console.error(error);
  process.exit(-1);
});

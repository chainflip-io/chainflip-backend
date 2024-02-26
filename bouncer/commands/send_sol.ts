#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund the solana address provided as the first argument with the amount
// provided in the second argument. The asset amount is interpreted in SOL
//
// For example: ./commands/send_sol.ts 7QQGNm3ptwinipDCyaCF7jY5katgmFUu1ieP2f7nwLpE 1.2
// will send 1.2 SOL to account 7QQGNm3ptwinipDCyaCF7jY5katgmFUu1ieP2f7nwLpE
// It also accepts non-encoded bs58 address representations:
// ./commands/send_sol.ts 0x2f3fcadf740018f6037513959bab60d0dbef26888d264d54fc4d3d36c8cf5c91 1.2

import { runWithTimeout } from '../shared/utils';
import { sendSol } from '../shared/send_sol';

async function main() {
  const solanaAddress = process.argv[2];
  const solAmount = process.argv[3].trim();

  console.log('Transferring ' + solAmount + ' SOL to ' + solanaAddress);
  await sendSol(solanaAddress, solAmount);

  process.exit(0);
}

runWithTimeout(main(), 20000).catch((error) => {
  console.error(error);
  process.exit(-1);
});

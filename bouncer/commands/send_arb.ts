#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund the arbitrum address provided as the first argument with the amount
// provided in the second argument. The asset amount is interpreted in ARB
//
// For example: ./commands/send_arb.ts 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6 1.2
// will send 1.2 ARB to account 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6

import { runWithTimeout } from '../shared/utils';
import { sendEvmNative } from '../shared/send_evm';

async function main() {
  const arbitrumAddress = process.argv[2];
  const arbAmount = process.argv[3].trim();

  console.log('Transferring ' + arbAmount + ' ARB to ' + arbitrumAddress);
  await sendEvmNative('Arbitrum', arbitrumAddress, arbAmount);

  process.exit(0);
}

runWithTimeout(main(), 20000).catch((error) => {
  console.error(error);
  process.exit(-1);
});

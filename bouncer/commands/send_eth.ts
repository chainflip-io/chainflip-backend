#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund the ethereum address provided as the first argument with the amount
// provided in the second argument. The asset amount is interpreted in Eth
//
// For example: ./commands/send_eth.ts 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6 1.2
// will send 1.2 Eth to account 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6

import { runWithTimeout } from '../shared/utils';
import { sendEvmNative } from '../shared/send_evm';

async function main() {
  const ethereumAddress = process.argv[2];
  const ethAmount = process.argv[3].trim();

  console.log('Transferring ' + ethAmount + ' Eth to ' + ethereumAddress);
  await sendEvmNative('Ethereum', ethereumAddress, ethAmount);

  process.exit(0);
}

runWithTimeout(main(), 20000).catch((error) => {
  console.error(error);
  process.exit(-1);
});

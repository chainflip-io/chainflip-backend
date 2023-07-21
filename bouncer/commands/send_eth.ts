#!/usr/bin/env pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund the ethereum address provided as the first argument with the amount
// provided in the second argument. The asset amount is interpreted in ETH
//
// For example: ./commands/fund_eth.ts 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6 1.2
// will send 1.2 ETH to account 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6

import { runWithTimeout } from '../shared/utils';
import { sendEth } from '../shared/send_eth';

async function main() {
  const ethereumAddress = process.argv[2];
  const ethAmount = process.argv[3].trim();

  console.log('Transferring ' + ethAmount + ' ETH to ' + ethereumAddress);
  await sendEth(ethereumAddress, ethAmount);

  process.exit(0);
}

runWithTimeout(main(), 20000).catch((error) => {
  console.error(error);
  process.exit(-1);
});

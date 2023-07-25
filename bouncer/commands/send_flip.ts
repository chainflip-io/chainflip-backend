#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will send FLIP to the ethereum address provided as the first argument with the amount
// provided in the second argument. The asset amount is interpreted as FLIP
//
// For example: ./commands/send_flip.ts 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6 5.5
// will send 5.5 FLIP to the account with address 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6

import { runWithTimeout } from '../shared/utils';
import { sendErc20 } from '../shared/send_erc20';

async function main(): Promise<void> {
  const ethereumAddress = process.argv[2];
  const flipAmount = process.argv[3].trim();

  await sendErc20(ethereumAddress, 'FLIP', flipAmount);

  process.exit(0);
}

runWithTimeout(main(), 50000).catch((error) => {
  console.error(error);
  process.exit(-1);
});

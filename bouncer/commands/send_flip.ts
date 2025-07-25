#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will send Flip to the ethereum address provided as the first argument with the amount
// provided in the second argument. The asset amount is interpreted as Flip
//
// For example: ./commands/send_flip.ts 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6 5.5
// will send 5.5 Flip to the account with address 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6

import { runWithTimeoutAndExit, getContractAddress } from 'shared/utils';
import { sendErc20 } from 'shared/send_erc20';
import { globalLogger } from 'shared/utils/logger';

async function main(): Promise<void> {
  const ethereumAddress = process.argv[2];
  const flipAmount = process.argv[3].trim();

  const contractAddress = getContractAddress('Ethereum', 'Flip');
  await sendErc20(globalLogger, 'Ethereum', ethereumAddress, contractAddress, flipAmount);
}

await runWithTimeoutAndExit(main(), 50);

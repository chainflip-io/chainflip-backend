#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund the arbitrum address provided as the first argument with the amount
// provided in the second argument. The asset amount is interpreted in Arb
//
// For example: ./commands/send_arbeth.ts 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6 1.2
// will send 1.2 Arb to account 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6

import { runWithTimeoutAndExit } from 'shared/utils';
import { sendEvmNative } from 'shared/send_evm';
import { globalLogger } from 'shared/utils/logger';

async function main() {
  const arbitrumAddress = process.argv[2];
  const arbAmount = process.argv[3].trim();

  globalLogger.info(`Transferring ${arbAmount} ArbEth to ${arbitrumAddress}`);
  await sendEvmNative(globalLogger, 'Arbitrum', arbitrumAddress, arbAmount);
}

await runWithTimeoutAndExit(main(), 20);

#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will set the minimum deposit amount of the asset defined by the first argument to the value
// given by the second argument. Using an amount of zero will remove the minimum deposit limitation
// so that any deposit is allowed.
//
// For example: ./commands/set_minimum_deposit.ts ETH 0.01
// will reject any ETH deposit below 0.01 ETH.

import { runWithTimeoutAndExit, parseAssetString, amountToFineAmountBigInt } from '../shared/utils';
import { setMinimumDeposit } from '../shared/set_minimum_deposit';
import { globalLogger } from '../shared/utils/logger';

async function main() {
  const asset = parseAssetString(process.argv[2]);
  const amount = amountToFineAmountBigInt(process.argv[3].trim(), asset);
  await setMinimumDeposit(globalLogger, asset, amount);
}

await runWithTimeoutAndExit(main(), 120);

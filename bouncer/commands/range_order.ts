#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will create a zero to infinity range order for the currency and amount given
// For example: ./commands/range_order.ts Btc 10

import { InternalAsset } from '@chainflip/cli';
import { rangeOrder } from '../shared/range_order';
import { parseAssetString, runWithTimeoutAndExit } from '../shared/utils';
import { globalLogger } from '../shared/utils/logger';

async function main() {
  const ccy = parseAssetString(process.argv[2]);
  const amount = parseFloat(process.argv[3].trim());
  await rangeOrder(globalLogger, ccy as InternalAsset, amount);
}

await runWithTimeoutAndExit(main(), 120);

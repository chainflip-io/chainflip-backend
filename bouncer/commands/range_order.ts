#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will create a zero to infinity range order for the currency and amount given
// For example: ./commands/range_order.ts Btc 10

import { InternalAsset } from '@chainflip/cli';
import { rangeOrder } from 'shared/range_order';
import { parseAssetString, runWithTimeoutAndExit } from 'shared/utils';
import { globalLogger } from 'shared/utils/logger';
import { fullAccountFromUri, newChainflipIO } from 'shared/utils/chainflip_io';

async function main() {
  const ccy = parseAssetString(process.argv[2]);
  const amount = parseFloat(process.argv[3].trim());

  const parentCf = await newChainflipIO(globalLogger, []);
  const cf = parentCf.with({ account: fullAccountFromUri('//LP_1', 'LP') });
  await rangeOrder(cf, ccy as InternalAsset, amount);
}

await runWithTimeoutAndExit(main(), 120);

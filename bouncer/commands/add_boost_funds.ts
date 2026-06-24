#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes 2 arguments:
// 1 - Amount
// 2 (optional) - Account URI (Default: "//LP_BOOST")
//
// Adds existing funds to the BTC 5bps boost pool and waits until it is confirmed via an event.
// For example: ./commands/add_boost_funds.ts 0.1 "//LP_2"

import { runWithTimeoutAndExit } from 'shared/utils';
import { addBoostFunds } from 'tests/boost';
import { globalLogger } from 'shared/utils/logger';
import { fullAccountFromUri, newChainflipIO } from 'shared/utils/chainflip_io';

const cf = await newChainflipIO(globalLogger, {
  account: fullAccountFromUri((process.argv[3] as `//LP${string}`) ?? '//LP_BOOST', 'LP'),
});

await runWithTimeoutAndExit(addBoostFunds(cf, Number(process.argv[2])), 80);

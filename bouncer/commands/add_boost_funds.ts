#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes 4 arguments:
// 1 - Asset
// 2 - Tier
// 3 - Amount
// 4 (optional) - Account URI (Default: "//LP_BOOST")
//
// Adds existing funds to the specified boost pool and waits until it is confirmed via an event.
// For example: ./commands/add_boost_funds.ts Btc 5 0.1 "//LP_2"

import { InternalAsset as Asset } from '@chainflip/cli';
import { runWithTimeoutAndExit } from 'shared/utils';
import { addBoostFunds } from 'tests/boost';
import { globalLogger } from 'shared/utils/logger';
import { ChainflipIO, fullAccountFromUri } from 'shared/utils/chainflip_io';

const cf = new ChainflipIO({
  account: fullAccountFromUri((process.argv[5] as `//LP${string}`) ?? '//LP_BOOST', 'LP'),
});

await runWithTimeoutAndExit(
  addBoostFunds(
    cf,
    globalLogger,
    process.argv[2] as Asset,
    Number(process.argv[3]),
    Number(process.argv[4]),
  ),
  80,
);

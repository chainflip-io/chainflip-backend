#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes 3 arguments:
// 1 - Asset
// 2 - Tier
// 3 (optional) - Account URI (Default: "//LP_BOOST")
//
// Stops boosting for the specified boost pool, then waits for the `StoppedBoosting` event and prints it.
// For example: ./commands/stop_boosting.ts Btc 5 "//LP_2"

import { InternalAsset as Asset } from '@chainflip/cli';
import { runWithTimeoutAndExit } from 'shared/utils';
import { stopBoosting } from 'tests/boost';
import { globalLogger } from 'shared/utils/logger';
import { ChainflipIO, fullAccountFromUri } from 'shared/utils/chainflip_io';

const cf = new ChainflipIO({
  account: fullAccountFromUri((process.argv[5] as `//LP${string}`) ?? '//LP_BOOST', 'Lp'),
});

async function main(): Promise<void> {
  const event = await stopBoosting(
    cf,
    globalLogger,
    process.argv[2] as Asset,
    Number(process.argv[3]),
  );
  globalLogger.info(`Stopped boosting event: ${JSON.stringify(event)}`);
}

await runWithTimeoutAndExit(main(), 30);

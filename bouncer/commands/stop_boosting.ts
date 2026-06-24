#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes 1 argument:
// 1 (optional) - Account URI (Default: "//LP_BOOST")
//
// Stops boosting BTC at the 5bps tier, then waits for the `StoppedBoosting` event and prints it.
// For example: ./commands/stop_boosting.ts "//LP_2"

import { runWithTimeoutAndExit } from 'shared/utils';
import { stopBoosting } from 'tests/boost';
import { globalLogger } from 'shared/utils/logger';
import { newChainflipIO, fullAccountFromUri } from 'shared/utils/chainflip_io';

const cf = await newChainflipIO(globalLogger, {
  account: fullAccountFromUri((process.argv[2] as `//LP${string}`) ?? '//LP_BOOST', 'LP'),
});

async function main(): Promise<void> {
  const event = await stopBoosting(cf);
  globalLogger.info(`Stopped boosting event: ${JSON.stringify(event)}`);
}

await runWithTimeoutAndExit(main(), 30);

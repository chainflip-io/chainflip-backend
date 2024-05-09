#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes 3 arguments:
// 1 - Asset
// 2 - Tier
// 3 (optional) - Account URI (Default: "//LP_1")
//
// Stops boosting for the specified boost pool, then waits for the `StoppedBoosting` event and prints it.
// For example: ./commands/stop_boosting.ts Btc 5 "//LP_2"

import { InternalAsset as Asset } from '@chainflip/cli/.';
import { runWithTimeout } from '../shared/utils';
import { stopBoosting } from '../shared/boost';

async function main(): Promise<void> {
  const event = await stopBoosting(
    process.argv[2] as Asset,
    Number(process.argv[3]),
    process.argv[4],
    true,
  );
  console.log(`Stopped boosting event: ${JSON.stringify(event)}`);
  process.exit(0);
}

runWithTimeout(main(), 30000).catch((error) => {
  console.error(error);
  process.exit(-1);
});

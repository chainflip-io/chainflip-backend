#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
// Starts (or restarts) the Broker API, LP API, and Deposit Monitor against a running localnet.
// Useful after a binary or runtime upgrade where these services need to be restarted
// to pick up new binaries or decode the updated runtime metadata.
//
// Args:
// --bins <path>: Directory containing the chainflip-broker-api and chainflip-lp-api binaries.
// --localnet_init <path>: Path to the localnet init directory. Defaults to ./localnet/init.
//
// Example:
// ./commands/start_dm_localnet_apis.ts --bins ../upgrade-to-bins
// ./commands/start_dm_localnet_apis.ts --bins ./target/release --localnet_init ./localnet/init

import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import { restartDepositMonitorAndLpAndBrokerApi } from 'shared/upgrade_network';
import { runWithTimeoutAndExit } from 'shared/utils';

async function main(): Promise<void> {
  const argv = await yargs(hideBin(process.argv))
    .option('bins', {
      describe: 'Directory containing the broker-api and lp-api binaries',
      type: 'string',
      demandOption: true,
      requiresArg: true,
    })
    .option('localnet_init', {
      describe: 'Path to the localnet init directory',
      type: 'string',
      default: './localnet/init',
    })
    .help().argv;

  await restartDepositMonitorAndLpAndBrokerApi(argv.localnet_init, argv.bins);
}

await runWithTimeoutAndExit(main(), 5 * 60);

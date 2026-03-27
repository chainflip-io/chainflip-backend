#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
// Submits a runtime upgrade to a running localnet.
//
// Args:
// --runtime <path>: Path to the runtime wasm file. (required)
// --semver_restriction <json>: JSON semver restriction, e.g. '{"major":1,"minor":2,"patch":3}'. Optional.
// --percent_nodes <number>: Percentage of nodes that must be on the new binary before the upgrade proceeds. Optional.
// --try_runtime: Run try-runtime checks before submitting the upgrade. Defaults to false.
//
// Examples:
// ./commands/submit_runtime_upgrade.ts --runtime ./state_chain_runtime.compact.compressed.wasm
// ./commands/submit_runtime_upgrade.ts --runtime ./runtime.wasm --semver_restriction '{"major":1,"minor":2,"patch":3}' --percent_nodes 50 --try_runtime

import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import { submitRuntimeUpgradeWithRestrictions } from 'shared/submit_runtime_upgrade';
import { runWithTimeoutAndExit } from 'shared/utils';
import { globalLogger } from 'shared/utils/logger';
import { newChainflipIO } from 'shared/utils/chainflip_io';

async function main() {
  const argv = await yargs(hideBin(process.argv))
    .option('runtime', {
      describe: 'Path to the runtime wasm file',
      type: 'string',
      demandOption: true,
      requiresArg: true,
    })
    .option('semver_restriction', {
      describe: 'JSON semver restriction e.g. \'{"major":1,"minor":2,"patch":3}\'',
      type: 'string',
    })
    .option('percent_nodes', {
      describe: 'Percentage of nodes that must be on the new binary before the upgrade proceeds',
      type: 'number',
    })
    .option('try_runtime', {
      describe: 'Run try-runtime checks before submitting the upgrade',
      type: 'boolean',
      default: false,
    })
    .help().argv;

  const semverRestriction = argv.semver_restriction
    ? JSON.parse(argv.semver_restriction)
    : undefined;

  const cf = await newChainflipIO(globalLogger, []);
  await submitRuntimeUpgradeWithRestrictions(
    cf,
    argv.runtime,
    semverRestriction,
    argv.percent_nodes,
    argv.try_runtime,
  );
}

await runWithTimeoutAndExit(main(), 20);

#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument: the epoch at which the FLIP 2.1 fee reward system should
// activate. It sets `pallet_cf_flip::FeeRewardsActivationEpoch` via a governance extrinsic.
//
// The argument is either an absolute epoch index, or a `+N` offset relative to the current epoch.
//
// For example: ./commands/set_fee_rewards_activation_epoch.ts 42
//              ./commands/set_fee_rewards_activation_epoch.ts +1
//
// Note that setting an epoch that is already in the past activates the reward system immediately.

import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { runWithTimeoutAndExit, sleep } from 'shared/utils';
import { getChainflipApi } from 'shared/utils/substrate';
import { globalLogger as logger } from 'shared/utils/logger';

async function main() {
  const arg = process.argv[2]?.trim();
  if (!arg) {
    throw new Error('Missing argument: expected an epoch index (e.g. `42`) or an offset (`+1`).');
  }

  await using chainflip = await getChainflipApi();
  const currentEpoch = await chainflip.query.validator.currentEpoch();

  const relative = arg.startsWith('+');
  const parsed = Number(relative ? arg.slice(1) : arg);
  if (!Number.isInteger(parsed) || parsed < 0) {
    throw new Error(`Invalid epoch argument: ${arg}`);
  }
  const activationEpoch = relative ? currentEpoch + parsed : parsed;

  logger.info(
    `Current epoch is ${currentEpoch}, setting FeeRewardsActivationEpoch to ${activationEpoch}`,
  );

  await submitGovernanceExtrinsic(
    (api) =>
      api.tx.flip.updatePalletConfig([
        { type: 'SetFeeRewardsActivationEpoch', value: activationEpoch },
      ]),
    logger,
  );

  // Automatic governance execution happens in a later block than the proposal, so poll until the
  // storage reflects the new value.
  let stored = await chainflip.query.flip.feeRewardsActivationEpoch();
  for (let attempt = 0; stored !== activationEpoch && attempt < 10; attempt++) {
    await sleep(6_000);
    stored = await chainflip.query.flip.feeRewardsActivationEpoch();
  }
  if (stored !== activationEpoch) {
    throw new Error(
      `FeeRewardsActivationEpoch was not updated: expected ${activationEpoch}, got ${stored}`,
    );
  }

  logger.info(`FeeRewardsActivationEpoch is now ${stored}`);
}

await runWithTimeoutAndExit(main(), 60);

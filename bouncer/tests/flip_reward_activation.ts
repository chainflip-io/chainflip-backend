import assert from 'assert';
import { sleep } from 'shared/utils';
import { testSwap } from 'shared/swapping';
import { getChainflipApi } from 'shared/utils/substrate';
import { newChainflipIO } from 'shared/utils/chainflip_io';
import { TestContext } from 'shared/utils/test_context';
import { flipPalletConfigUpdatedEvent } from 'generated/events/flip/palletConfigUpdated';
import { flipFlipDistributedEvent } from 'generated/events/flip/flipDistributed';
import { emissionsSupplyUpdateBroadcastRequestedEvent } from 'generated/events/emissions/supplyUpdateBroadcastRequested';
import { validatorNewEpochEvent } from 'generated/events/validator/newEpoch';

const SWAP_FEE_VOLUME_COUNT = 5;

async function getCurrentEpoch(): Promise<number> {
  await using chainflip = await getChainflipApi();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return ((await chainflip.query.validator.currentEpoch()) as any).toNumber();
}

async function getFeeRewardsActivationEpoch(): Promise<number> {
  await using chainflip = await getChainflipApi();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return ((await chainflip.query.flip.feeRewardsActivationEpoch()) as any).toNumber();
}

async function getTotalIssuance(): Promise<bigint> {
  await using chainflip = await getChainflipApi();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return ((await chainflip.query.flip.totalIssuance()) as any).toBigInt();
}

// Tests the activation flow of the FLIP 2.1 reward system: governance sets an activation epoch
// ahead of the current one, nothing changes until that epoch is reached, and once it is, fee
// rewards are distributed to authorities instead of burned, fixing the total FLIP supply (aside
// from the one-off forced supply sync that fires in the activation epoch itself).
export async function testFlipRewardActivation(testContext: TestContext) {
  const logger = testContext.logger;
  const cf = await newChainflipIO(logger, {});

  const activationEpoch = (await getCurrentEpoch()) + 1;
  logger.info(`Setting FLIP reward activation epoch to ${activationEpoch} via governance`);

  await cf.submitGovernance({
    extrinsic: (api) =>
      api.tx.flip.updatePalletConfig([{ SetFeeRewardsActivationEpoch: activationEpoch }]),
    expectedEvent: flipPalletConfigUpdatedEvent.refine(
      (event) =>
        event.update.__kind === 'SetFeeRewardsActivationEpoch' &&
        event.update.value === activationEpoch,
    ),
  });

  const storedActivationEpoch = await getFeeRewardsActivationEpoch();
  assert.strictEqual(
    storedActivationEpoch,
    activationEpoch,
    'FeeRewardsActivationEpoch storage was not updated by the governance call',
  );

  // Pre-activation, per-block authority emissions are still minted, so total issuance keeps
  // growing.
  const issuanceBeforeActivation = await getTotalIssuance();
  await sleep(12_000);
  const issuanceStillPreActivation = await getTotalIssuance();
  assert.ok(
    issuanceStillPreActivation > issuanceBeforeActivation,
    `Expected FLIP total issuance to increase before activation (block emissions), but went from ${issuanceBeforeActivation} to ${issuanceStillPreActivation}`,
  );

  logger.info('Forcing a rotation to reach the activation epoch');
  await cf.submitGovernance({ extrinsic: (api) => api.tx.validator.forceRotation() });
  const epochAtActivation = await cf.stepUntilEvent(validatorNewEpochEvent);
  assert.strictEqual(
    epochAtActivation,
    activationEpoch,
    `Expected the forced rotation to land exactly on the activation epoch ${activationEpoch}, got ${epochAtActivation}`,
  );

  // Reaching the activation epoch triggers a one-off forced supply sync in place of the
  // (now disabled) periodic burn-and-broadcast cycle.
  await cf.expectEvent(emissionsSupplyUpdateBroadcastRequestedEvent);
  logger.info(`Reached FLIP reward activation epoch ${epochAtActivation}`);

  const issuanceAtActivation = await getTotalIssuance();

  // Generate some real swap fee volume during the activation epoch, so that the reserve has a
  // non-zero balance to distribute at the next rotation.
  logger.info(`Generating swap fee volume post-activation: ${SWAP_FEE_VOLUME_COUNT}x Eth->Btc`);
  await cf.all(
    Array.from(
      { length: SWAP_FEE_VOLUME_COUNT },
      () => (subcf) => testSwap(subcf, 'Eth', 'Btc', undefined, undefined, testContext.swapContext),
    ),
  );

  // Post-activation there's no more block emission minting and no more periodic fee burning, so
  // total issuance stays fixed even though real fee-generating swap volume just went through.
  const issuanceAfterSwaps = await getTotalIssuance();
  assert.strictEqual(
    issuanceAfterSwaps,
    issuanceAtActivation,
    `Expected FLIP total issuance to stay fixed after activation, but went from ${issuanceAtActivation} to ${issuanceAfterSwaps}`,
  );

  logger.info('Forcing a second rotation to trigger the first FLIP reward distribution');
  await cf.submitGovernance({ extrinsic: (api) => api.tx.validator.forceRotation() });
  const epochAfterActivation = await cf.stepUntilEvent(validatorNewEpochEvent);
  assert.strictEqual(epochAfterActivation, activationEpoch + 1);

  // The epoch following activation distributes accrued fee rewards to authorities. With real
  // swap volume behind it, a non-zero amount should actually have been distributed.
  const distributed = await cf.expectEvent(flipFlipDistributedEvent);
  const totalDistributed = distributed.amounts.reduce((sum, [, amount]) => sum + amount, 0n);
  assert.ok(
    totalDistributed > 0n,
    `Expected a non-zero FLIP reward distribution after generating swap fee volume, got ${totalDistributed}`,
  );

  // Distribution moves FLIP out of the on-chain reserve to authorities without minting or
  // burning, so total issuance is still unaffected.
  const issuanceAfterDistribution = await getTotalIssuance();
  assert.strictEqual(
    issuanceAfterDistribution,
    issuanceAtActivation,
    `Expected FLIP total issuance to remain fixed through reward distribution, but went from ${issuanceAtActivation} to ${issuanceAfterDistribution}`,
  );

  logger.info('FLIP reward system activated successfully with fixed total issuance');
}

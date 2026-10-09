import assert from 'assert';
import { testSwap } from 'shared/swapping';
import { getChainflipApi } from 'shared/utils/substrate';
import { newChainflipIO } from 'shared/utils/chainflip_io';
import { TestContext } from 'shared/utils/test_context';
import { flipFlipDistributedEvent } from 'generated/events/flip/flipDistributed';
import { validatorNewEpochEvent } from 'generated/events/validator/newEpoch';

const SWAP_FEE_VOLUME_COUNT = 5;

async function getTotalIssuance(): Promise<bigint> {
  await using chainflip = await getChainflipApi();
  return chainflip.query.flip.totalIssuance();
}

// Tests the FLIP 2.1 reward system: fee rewards accrued during an epoch are distributed to
// authorities (rather than burned) at the following epoch transition, and total FLIP supply
// stays fixed throughout.
export async function testFlipRewardDistribution(testContext: TestContext) {
  const logger = testContext.logger;
  const cf = await newChainflipIO(logger, {});

  const issuanceBeforeSwaps = await getTotalIssuance();

  // Generate some real swap fee volume, so that the reserve has a non-zero balance to
  // distribute at the next rotation.
  logger.info(`Generating swap fee volume: ${SWAP_FEE_VOLUME_COUNT}x Eth->Btc`);
  await cf.all(
    Array.from(
      { length: SWAP_FEE_VOLUME_COUNT },
      () => (subcf) => testSwap(subcf, 'Eth', 'Btc', undefined, undefined, testContext.swapContext),
    ),
  );

  // There's no more block emission minting and no more periodic fee burning, so total issuance
  // stays fixed even though real fee-generating swap volume just went through.
  const issuanceAfterSwaps = await getTotalIssuance();
  assert.strictEqual(
    issuanceAfterSwaps,
    issuanceBeforeSwaps,
    `Expected FLIP total issuance to stay fixed, but went from ${issuanceBeforeSwaps} to ${issuanceAfterSwaps}`,
  );

  logger.info('Forcing a rotation to trigger the FLIP reward distribution');
  await cf.submitGovernance({ extrinsic: (api) => api.tx.validator.forceRotation() });
  await cf.stepUntilEvent(validatorNewEpochEvent);

  // The fee rewards accrued during the epoch are distributed at its end, in the same block as
  // the transition to the next epoch. With real swap volume behind it, a non-zero amount should
  // actually have been distributed.
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
    issuanceBeforeSwaps,
    `Expected FLIP total issuance to remain fixed through reward distribution, but went from ${issuanceBeforeSwaps} to ${issuanceAfterDistribution}`,
  );

  logger.info('FLIP reward distributed successfully with fixed total issuance');
}

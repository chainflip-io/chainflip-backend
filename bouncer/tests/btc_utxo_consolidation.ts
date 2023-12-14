#!/usr/bin/env -S pnpm tsx
import assert from 'assert';

import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { provideLiquidity } from '../shared/provide_liquidity';
import { getChainflipApi, runWithTimeout, sleep } from '../shared/utils';

const chainflip = await getChainflipApi();

async function queryUtxos(): Promise<{ amount: number; count: number }> {
  const utxos: [{ amount: number }] = (
    await chainflip.query.environment.bitcoinAvailableUtxos()
  ).toJSON();

  return {
    amount: utxos.reduce((acc, utxo) => acc + utxo.amount, 0),
    count: utxos.length,
  };
}

async function test() {
  const initialUtxos = await queryUtxos();

  console.log(`Initial utxo count: ${initialUtxos.count}`);

  if (initialUtxos.count === 0) {
    throw new Error('Test precondition violated: btc vault should have at least 1 utxo');
  }

  // Reset consolidation parameters to ensure consolidation doesn't trigger immediately:
  await submitGovernanceExtrinsic(
    chainflip.tx.environment.updateConsolidationParameters({
      consolidationSize: 100,
      consolidationThreshold: 200,
    }),
  );

  // Setting the threshold to current utxo count + 2 allows us to test the more
  // general case of consolidationSize != consolidationThreshold even when there
  // is only 1 UTXO available initially
  const consolidationSize = initialUtxos.count + 1;
  const consolidationThreshold = initialUtxos.count + 2;

  await provideLiquidity('BTC', 2);
  await provideLiquidity('BTC', 3);

  const amountBeforeConsolidation = (await queryUtxos()).amount;
  console.log(`Total amount in BTC vault is: ${amountBeforeConsolidation}`);

  console.log(
    `Setting consolidation threshold to: ${consolidationThreshold} and size to: ${consolidationSize}`,
  );

  await submitGovernanceExtrinsic(
    chainflip.tx.environment.updateConsolidationParameters({
      consolidationSize,
      consolidationThreshold,
    }),
  );

  // The above should trigger consolidation after which we should have exactly 2 UTXOs
  for (let i = 0; i < 100; i++) {
    await sleep(1000);
    const utxos = await queryUtxos();

    if (utxos.count === 2) {
      const amountAfterConsolidation = utxos.amount;
      console.log(`Total amount in BTC vault after consolidation: ${amountAfterConsolidation}`);

      const ERROR_MARGIN = 1000000;

      // Ensure our balance is mostly unchanged after consolidation (taking tx fees into account):
      assert(
        amountAfterConsolidation <= amountBeforeConsolidation &&
          amountAfterConsolidation > amountBeforeConsolidation - ERROR_MARGIN,
      );
      break;
    }

    console.log(`Waiting util UTXO count is 2 (currently: ${utxos.count})`);
  }

  // Clean up after the test to minimise conflicts with any other tests
  await submitGovernanceExtrinsic(
    chainflip.tx.environment.updateConsolidationParameters({
      consolidationSize: 100,
      consolidationThreshold: 200,
    }),
  );

  process.exit(0);
}

runWithTimeout(test(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});

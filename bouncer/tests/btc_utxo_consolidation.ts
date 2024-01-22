#!/usr/bin/env -S pnpm tsx
import assert from 'assert';

import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { provideLiquidity } from '../shared/provide_liquidity';
import { getChainflipApi, observeEvent, runWithTimeout } from '../shared/utils';

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
  console.log('=== Testing BTC UTXO Consolidation ===');
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

  // Add 2 utxo which should later trigger consolidation as per the parameters above:
  await provideLiquidity('BTC', 2);
  await provideLiquidity('BTC', 3);

  const amountBeforeConsolidation = (await queryUtxos()).amount;
  console.log(`Total amount in BTC vault is: ${amountBeforeConsolidation}`);

  console.log(
    `Setting consolidation threshold to: ${consolidationThreshold} and size to: ${consolidationSize}`,
  );

  const consolidationEventPromise = observeEvent(
    'bitcoinIngressEgress:UtxoConsolidation',
    chainflip,
  );

  // We should have exactly consolidationThreshold utxos,
  // so this should trigger consolidation:
  await submitGovernanceExtrinsic(
    chainflip.tx.environment.updateConsolidationParameters({
      consolidationSize,
      consolidationThreshold,
    }),
  );

  console.log(`Waiting for the consolidation event`);
  const consolidationBroadcastId = (await consolidationEventPromise).data.broadcastId;
  console.log(`Consolidation event is observed! Broadcast id: ${consolidationBroadcastId}`);

  const broadcastSuccessPromise = observeEvent(
    'bitcoinBroadcaster:BroadcastSuccess',
    chainflip,
    (event) => {
      if (consolidationBroadcastId === event.data.broadcastId) return true;
      return false;
    },
  );
  const feeDeficitPromise = observeEvent(
    'bitcoinBroadcaster:TransactionFeeDeficitRecorded',
    chainflip,
  );

  console.log(`Waiting for broadcast ${consolidationBroadcastId} to succeed`);
  await broadcastSuccessPromise;
  console.log(`Broadcast ${consolidationBroadcastId} is successful!`);

  const feeDeficit = (await feeDeficitPromise).data.amount;
  console.log(`Fee deficit: ${feeDeficit}`);

  // After consolidation we should have exactly 2 UTXOs
  // with the total amount unchanged (minus fees):
  const utxos = await queryUtxos();

  console.log(`Total utxo count after consolidation: ${utxos.count} (amount: ${utxos.amount})`);
  assert(utxos.count === 2, 'should have 2 total utxos');
  assert(
    utxos.amount === amountBeforeConsolidation - Number(feeDeficit),
    'total BTC amount should remain unchanged',
  );

  // Clean up after the test to minimise conflicts with any other tests
  await submitGovernanceExtrinsic(
    chainflip.tx.environment.updateConsolidationParameters({
      consolidationSize: 100,
      consolidationThreshold: 200,
    }),
  );

  console.log('=== BTC UTXO Consolidation test completed ===');

  process.exit(0);
}

runWithTimeout(test(), 200000).catch((error) => {
  console.error(error);
  process.exit(-1);
});

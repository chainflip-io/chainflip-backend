import assert from 'assert';

import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { newChainflipIO } from 'shared/utils/chainflip_io';
import { observeEvent, getChainflipApi } from 'shared/utils/substrate';
import { TestContext } from 'shared/utils/test_context';

interface Utxo {
  id: string;
  amount: number;
  depositAddress: string;
}

async function queryUtxos(): Promise<{ amount: number; count: number }> {
  await using chainflip = await getChainflipApi();
  const utxos: Utxo[] = JSON.parse(
    (await chainflip.query.environment.bitcoinAvailableUtxos()).toString(),
  );

  return {
    amount: utxos.reduce((acc, utxo) => acc + utxo.amount, 0),
    count: utxos.length,
  };
}

export async function testBtcUtxoConsolidation(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
  const initialUtxos = await queryUtxos();

  cf.debug(`Initial utxo count: ${initialUtxos.count}`);

  if (initialUtxos.count === 0) {
    throw new Error('Test precondition violated: btc vault should have at least 1 utxo');
  }

  // Reset consolidation parameters to ensure consolidation doesn't trigger immediately:
  await submitGovernanceExtrinsic((api) =>
    api.tx.environment.updateConsolidationParameters({
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
  await depositLiquidity(cf, 'Btc', 2);
  await depositLiquidity(cf, 'Btc', 3);

  const amountBeforeConsolidation = (await queryUtxos()).amount;
  cf.debug(`Total amount in BTC vault is: ${amountBeforeConsolidation}`);

  cf.debug(
    `Setting consolidation threshold to: ${consolidationThreshold} and size to: ${consolidationSize}`,
  );

  const consolidationEventPromise = observeEvent(
    cf.logger,
    'bitcoinIngressEgress:UtxoConsolidation',
  ).event;

  // We should have exactly consolidationThreshold utxos,
  // so this should trigger consolidation:
  await submitGovernanceExtrinsic((api) =>
    api.tx.environment.updateConsolidationParameters({
      consolidationSize,
      consolidationThreshold,
    }),
  );

  cf.debug(`Waiting for the consolidation event`);
  const consolidationBroadcastId = (await consolidationEventPromise).data.broadcastId;
  cf.debug(`Consolidation event is observed! Broadcast id: ${consolidationBroadcastId}`);

  const broadcastSuccessPromise = observeEvent(cf.logger, 'bitcoinBroadcaster:BroadcastSuccess', {
    test: (event) => consolidationBroadcastId === event.data.broadcastId,
  }).event;

  const feeDeficitPromise = observeEvent(
    cf.logger,
    'bitcoinBroadcaster:TransactionFeeDeficitRecorded',
  ).event;

  cf.debug(`Waiting for broadcast ${consolidationBroadcastId} to succeed`);
  await broadcastSuccessPromise;
  cf.debug(`Broadcast ${consolidationBroadcastId} is successful!`);

  const feeDeficit = (await feeDeficitPromise).data.amount;
  cf.debug(`Fee deficit: ${feeDeficit}`);

  // After consolidation we should have exactly 2 UTXOs
  // with the total amount unchanged (minus fees):
  const utxos = await queryUtxos();

  cf.debug(`Total utxo count after consolidation: ${utxos.count} (amount: ${utxos.amount})`);
  assert(utxos.count === 2, 'should have 2 total utxos');
  assert(
    utxos.amount === amountBeforeConsolidation - Number(feeDeficit),
    'total Btc amount should remain unchanged',
  );

  // Clean up after the test to minimise conflicts with any other tests
  await submitGovernanceExtrinsic((api) =>
    api.tx.environment.updateConsolidationParameters({
      consolidationSize: 100,
      consolidationThreshold: 200,
    }),
  );
}

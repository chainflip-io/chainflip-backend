import assert from 'assert';
import { ExecutableTest } from '../shared/executable_test';
import {
  amountToFineAmount,
  Asset,
  assetDecimals,
  createStateChainKeypair,
  newAddress,
  observeBalanceIncrease,
} from '../shared/utils';
import { getBalance } from '../shared/get_balance';
import { getEarnedBrokerFees } from './broker_fee_collection';
import {
  buildAndSendBtcVaultSwap,
  openPrivateBtcChannel,
  registerAffiliate,
} from '../shared/btc_vault_swap';
import { observeEvent } from '../shared/utils/substrate';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testVaultSwapFeeCollection = new ExecutableTest(
  'Vault-Swap-Fee-Collection',
  main,
  120,
);

// Fee to use for the broker and affiliates
const commissionBps = 100;

async function testVaultSwap(
  depositAmountBtc: number,
  brokerUri: string,
  destinationAsset: Asset,
  affiliateUri: string,
) {
  // Addresses
  const destinationAddress = await newAddress(destinationAsset, 'BTC_VAULT_SWAP');
  testVaultSwapFeeCollection.debugLog('destinationAddress:', destinationAddress);
  const refundAddress = await newAddress('Btc', 'BTC_VAULT_SWAP_REFUND');
  testVaultSwapFeeCollection.debugLog('Refund address:', refundAddress);

  // Amounts before swap
  const destinationAmountBeforeSwap = await getBalance(destinationAsset, destinationAddress);
  const broker = createStateChainKeypair(brokerUri);
  const affiliate = createStateChainKeypair(affiliateUri);
  const earnedBrokerFeesBefore = await getEarnedBrokerFees(broker);
  const earnedAffiliateFeesBefore = await getEarnedBrokerFees(affiliate);
  testVaultSwapFeeCollection.debugLog('Earned broker fees before:', earnedBrokerFeesBefore);
  testVaultSwapFeeCollection.debugLog('Earned affiliate fees before:', earnedAffiliateFeesBefore);

  const observeSwapExecutedEvent = observeEvent(`swapping:SwapExecuted`, {
    test: (event) =>
      event.data.inputAsset === 'Btc' &&
      event.data.outputAsset === destinationAsset &&
      event.data.inputAmount.replace(/,/g, '') ===
        amountToFineAmount(depositAmountBtc.toString(), assetDecimals('Btc')),
  }).event;

  await buildAndSendBtcVaultSwap(
    depositAmountBtc,
    brokerUri,
    destinationAsset,
    destinationAddress,
    refundAddress,
    [affiliate.address],
  );

  // Complete swap and check balance
  testVaultSwapFeeCollection.debugLog('Waiting for swap executed event');
  await observeSwapExecutedEvent;
  testVaultSwapFeeCollection.log(`Btc -> ${destinationAsset} Vault Swap executed`);
  await observeBalanceIncrease(destinationAsset, destinationAddress, destinationAmountBeforeSwap);
  testVaultSwapFeeCollection.log(`Balance increased, Vault Swap Complete`);

  // Check that both the broker and affiliate earned fees
  const earnedBrokerFeesAfter = await getEarnedBrokerFees(broker);
  const earnedAffiliateFeesAfter = await getEarnedBrokerFees(affiliate);
  testVaultSwapFeeCollection.debugLog('Earned broker fees after:', earnedBrokerFeesAfter);
  testVaultSwapFeeCollection.debugLog('Earned affiliate fees after:', earnedAffiliateFeesAfter);
  assert(earnedBrokerFeesAfter > earnedBrokerFeesBefore, 'No increase in earned broker fees');
  assert(
    earnedAffiliateFeesAfter > earnedAffiliateFeesBefore,
    'No increase in earned affiliate fees',
  );
}

async function main() {
  const btcDepositAmount = 0.1;
  // TODO: Fee collection will work properly when using 'BROKER_1' and 'BROKER_2' because it will be effected by the other tests.
  const brokerUri = '//BROKER_1';
  const affiliateUri = '//BROKER_2';

  await openPrivateBtcChannel(brokerUri);
  await registerAffiliate(brokerUri, affiliateUri, 0);
  await testVaultSwap(btcDepositAmount, brokerUri, 'Flip', affiliateUri);
}

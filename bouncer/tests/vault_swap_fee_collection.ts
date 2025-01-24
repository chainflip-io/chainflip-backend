import assert from 'assert';
import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import { ExecutableTest } from '../shared/executable_test';
import { createStateChainKeypair, defaultAssetAmounts, newAddress } from '../shared/utils';
import { getEarnedBrokerFees } from './broker_fee_collection';
import { openPrivateBtcChannel, registerAffiliate } from '../shared/btc_vault_swap';
import { setupBrokerAccount } from '../shared/setup_account';
import { performVaultSwap } from '../shared/perform_swap';
import { prepareSwap } from '../shared/swapping';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testVaultSwapFeeCollection = new ExecutableTest(
  'Vault-Swap-Fee-Collection',
  main,
  230,
);

// Fee to use for the broker and affiliates
const commissionBps = 100;

async function testFeeCollection(inputAsset: Asset) {
  testVaultSwapFeeCollection.debugLog('Testing:', inputAsset);

  // Setup broker accounts. Different for each asset and specific to this test.
  const brokerUri = `//BROKER_VAULT_FEE_COLLECTION_${inputAsset}`;
  const affiliateUri = `//BROKER_VAULT_FEE_COLLECTION_AFFILIATE_${inputAsset}`;
  const broker = createStateChainKeypair(brokerUri);
  const affiliate = createStateChainKeypair(affiliateUri);
  testVaultSwapFeeCollection.debugLog('Broker:', broker.address);
  testVaultSwapFeeCollection.debugLog('Affiliate:', affiliate.address);
  await Promise.all([setupBrokerAccount(brokerUri), setupBrokerAccount(affiliateUri)]);
  if (inputAsset === Assets.Btc) {
    await openPrivateBtcChannel(brokerUri);
  }
  const affiliateShotId = 0;
  testVaultSwapFeeCollection.debugLog('Registering affiliate');
  const event = await registerAffiliate(
    brokerUri,
    await newAddress('Eth', 'BTC_VAULT_SWAP_REFUND'),
  );

  const shortId = event.data.shortId as number;
  const affiliateId = event.data.affiliateId as string;

  // Setup
  const feeAsset = Assets.Usdc;
  const destAsset = inputAsset === feeAsset ? Assets.Flip : feeAsset;
  const depositAmount = defaultAssetAmounts(inputAsset);
  const { destAddress, tag } = await prepareSwap(
    inputAsset,
    feeAsset,
    undefined, // addressType
    undefined, // messageMetadata
    'VaultSwapFeeTest',
    testVaultSwapFeeCollection.debug,
    testVaultSwapFeeCollection.swapContext,
  );

  // Amounts before swap
  const earnedBrokerFeesBefore = await getEarnedBrokerFees(broker.address);
  const earnedAffiliateFeesBefore = await getEarnedBrokerFees(affiliateId);
  testVaultSwapFeeCollection.debugLog('Earned broker fees before:', earnedBrokerFeesBefore);
  testVaultSwapFeeCollection.debugLog('Earned affiliate fees before:', earnedAffiliateFeesBefore);

  // Do the vault swap
  await performVaultSwap(
    inputAsset,
    destAsset,
    destAddress,
    tag,
    undefined, // messageMetadata
    testVaultSwapFeeCollection.swapContext,
    testVaultSwapFeeCollection.debug,
    depositAmount,
    0, // boostFeeBps
    undefined, // fillOrKillParams
    undefined, // dcaParams
    { account: broker.address, commissionBps },
    [{ accountAddress: affiliateId, accountShortId: affiliateShotId, commissionBps }],
  );

  // Check that both the broker and affiliate earned fees
  const earnedBrokerFeesAfter = await getEarnedBrokerFees(broker.address);
  const earnedAffiliateFeesAfter = await getEarnedBrokerFees(affiliateId);
  testVaultSwapFeeCollection.debugLog('Earned broker fees after:', earnedBrokerFeesAfter);
  testVaultSwapFeeCollection.debugLog('Earned affiliate fees after:', earnedAffiliateFeesAfter);
  assert(
    earnedBrokerFeesAfter > earnedBrokerFeesBefore,
    `No increase in earned broker fees after ${inputAsset} swap`,
  );
  assert(
    earnedAffiliateFeesAfter > earnedAffiliateFeesBefore,
    `No increase in earned affiliate fees after ${inputAsset} swap`,
  );
}

async function main() {
  await Promise.all([
    testFeeCollection(Assets.Btc),
    testFeeCollection(Assets.Eth),
    testFeeCollection(Assets.ArbEth),
    testFeeCollection(Assets.Sol),
  ]);
}

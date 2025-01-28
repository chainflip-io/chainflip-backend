import assert from 'assert';
import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import { KeyringPair } from '@polkadot/keyring/types';
import { ExecutableTest } from '../shared/executable_test';
import {
  createStateChainKeypair,
  defaultAssetAmounts,
  handleSubstrateError,
  newAddress,
  sleep,
} from '../shared/utils';
import { getEarnedBrokerFees } from './broker_fee_collection';
import { openPrivateBtcChannel, registerAffiliate } from '../shared/btc_vault_swap';
import { setupBrokerAccount } from '../shared/setup_account';
import { performVaultSwap } from '../shared/perform_swap';
import { prepareSwap } from '../shared/swapping';
import { getChainflipApi } from '../shared/utils/substrate';
import { getBalance } from '../shared/get_balance';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testVaultSwapFeeCollection = new ExecutableTest(
  'Vault-Swap-Fee-Collection',
  main,
  600,
);

// Fee to use for the broker and affiliates
const commissionBps = 100;

async function testWithdrawCollectedAffiliateFees(
  broker: KeyringPair,
  affiliateAccountId: string,
  withdrawAddress: string,
) {
  const chainflip = await getChainflipApi();

  const balanceObserveTimeout = 60;
  let success = false;

  testVaultSwapFeeCollection.log('Starting withdraw collected affiliate fees test...');
  testVaultSwapFeeCollection.log('Affiliate account ID:', affiliateAccountId);
  testVaultSwapFeeCollection.log('Withdraw address:', withdrawAddress);

  await chainflip.tx.swapping
    .affiliateWithdrawalRequest(affiliateAccountId)
    .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip));

  testVaultSwapFeeCollection.log('Withdrawal request sent!');
  testVaultSwapFeeCollection.log(
    'Waiting for balance change... Observing address:',
    withdrawAddress,
  );

  // Wait for balance change
  for (let i = 0; i < balanceObserveTimeout; i++) {
    if ((await getBalance(Assets.Usdc, withdrawAddress)) !== '0') {
      success = true;
      break;
    }
    await sleep(1000);
  }

  assert(success, `Withdrawal failed - No balance change detected within the timeout period 🙅‍♂️.`);
  testVaultSwapFeeCollection.log('Withdrawal successful ✅.');
}

async function testFeeCollection(inputAsset: Asset): Promise<[KeyringPair, string, string]> {
  testVaultSwapFeeCollection.debugLog('Testing:', inputAsset);

  // Setup broker accounts. Different for each asset and specific to this test.
  const brokerUri = `//BROKER_VAULT_FEE_COLLECTION_${inputAsset}`;
  const affiliateUri = `//BROKER_VAULT_FEE_COLLECTION_AFFILIATE_${inputAsset}`;
  const broker = createStateChainKeypair(brokerUri);
  const affiliate = createStateChainKeypair(affiliateUri);
  const refundAddress = await newAddress('Eth', 'BTC_VAULT_SWAP_REFUND' + Math.random() * 100);
  testVaultSwapFeeCollection.debugLog('Broker:', broker.address);
  testVaultSwapFeeCollection.debugLog('Affiliate:', affiliate.address);
  await Promise.all([setupBrokerAccount(brokerUri), setupBrokerAccount(affiliateUri)]);
  if (inputAsset === Assets.Btc) {
    await openPrivateBtcChannel(brokerUri);
  }

  testVaultSwapFeeCollection.debugLog('Registering affiliate');
  const event = await registerAffiliate(brokerUri, refundAddress);

  const affiliateShotId = event.data.shortId as number;
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

  return Promise.resolve([broker, affiliateId, refundAddress]);
}

async function main() {
  await Promise.all([
    testFeeCollection(Assets.Eth),
    testFeeCollection(Assets.ArbEth),
    testFeeCollection(Assets.Sol),
  ]);

  // Test the affiliate withdrawal functionality
  const [broker, affiliateId, refundAddress] = await testFeeCollection(Assets.Btc);
  await testWithdrawCollectedAffiliateFees(broker, affiliateId, refundAddress);
}

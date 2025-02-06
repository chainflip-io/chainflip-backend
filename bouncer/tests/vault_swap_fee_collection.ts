import assert from 'assert';
import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
// eslint-disable-next-line no-restricted-imports
import type { KeyringPair } from '@polkadot/keyring/types';
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
import { TestContext } from '../shared/swap_context';
import { Logger } from '../shared/utils/logger';

// Fee to use for the broker and affiliates
const commissionBps = 100;

async function testWithdrawCollectedAffiliateFees(
  broker: KeyringPair,
  affiliateAccountId: string,
  withdrawAddress: string,
  logger: Logger,
) {
  const chainflip = await getChainflipApi();

  const balanceObserveTimeout = 60;
  let success = false;

  logger.info('Starting withdraw collected affiliate fees test...');
  logger.info('Affiliate account ID:', affiliateAccountId);
  logger.info('Withdraw address:', withdrawAddress);

  await chainflip.tx.swapping
    .affiliateWithdrawalRequest(affiliateAccountId)
    .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip));

  logger.info('Withdrawal request sent!');
  logger.info('Waiting for balance change... Observing address:', withdrawAddress);

  // Wait for balance change
  for (let i = 0; i < balanceObserveTimeout; i++) {
    if ((await getBalance(Assets.Usdc, withdrawAddress)) !== '0') {
      success = true;
      break;
    }
    await sleep(1000);
  }

  assert(success, `Withdrawal failed - No balance change detected within the timeout period 🙅‍♂️.`);
  logger.info('Withdrawal successful ✅.');
}

async function testFeeCollection(
  inputAsset: Asset,
  testContext: TestContext,
): Promise<[KeyringPair, string, string]> {
  const logger = testContext.logger.child({ asset: inputAsset });

  // Setup broker accounts. Different for each asset and specific to this test.
  const brokerUri = `//BROKER_VAULT_FEE_COLLECTION_${inputAsset}`;
  const broker = createStateChainKeypair(brokerUri);
  const refundAddress = await newAddress('Eth', 'BTC_VAULT_SWAP_REFUND' + Math.random() * 100);
  await Promise.all([setupBrokerAccount(brokerUri)]);
  if (inputAsset === Assets.Btc) {
    await openPrivateBtcChannel(brokerUri);
  }

  logger.debug('Registering affiliate');
  const event = await registerAffiliate(brokerUri, refundAddress);

  const affiliateId = event.data.affiliateId as string;

  logger.debug('Broker:', broker.address);
  logger.debug('Affiliate:', affiliateId);

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
    true, // log
    testContext.swapContext,
  );

  // Amounts before swap
  const earnedBrokerFeesBefore = await getEarnedBrokerFees(broker.address);
  const earnedAffiliateFeesBefore = await getEarnedBrokerFees(affiliateId);
  logger.debug('Earned broker fees before:', earnedBrokerFeesBefore);
  logger.debug('Earned affiliate fees before:', earnedAffiliateFeesBefore);

  // Do the vault swap
  await performVaultSwap(
    inputAsset,
    destAsset,
    destAddress,
    tag,
    undefined, // messageMetadata
    testContext.swapContext,
    true, // log
    depositAmount,
    0, // boostFeeBps
    undefined, // fillOrKillParams
    undefined, // dcaParams
    { account: broker.address, commissionBps },
    [{ accountAddress: affiliateId, commissionBps }],
  );

  // Check that both the broker and affiliate earned fees
  const earnedBrokerFeesAfter = await getEarnedBrokerFees(broker.address);
  const earnedAffiliateFeesAfter = await getEarnedBrokerFees(affiliateId);
  logger.debug('Earned broker fees after:', earnedBrokerFeesAfter);
  logger.debug('Earned affiliate fees after:', earnedAffiliateFeesAfter);
  assert(
    earnedBrokerFeesAfter > earnedBrokerFeesBefore,
    `No increase in earned broker fees after ${tag}(${inputAsset} -> ${destAsset}) vault swap: ${{ account: broker.address, commissionBps }}, ${earnedBrokerFeesBefore} -> ${earnedBrokerFeesAfter}`,
  );
  assert(
    earnedAffiliateFeesAfter > earnedAffiliateFeesBefore,
    `No increase in earned affiliate fees after ${inputAsset} swap`,
  );

  return Promise.resolve([broker, affiliateId, refundAddress]);
}

export async function testVaultSwapFeeCollection(testContext: TestContext) {
  await Promise.all([
    testFeeCollection(Assets.Eth, testContext),
    testFeeCollection(Assets.ArbEth, testContext),
    testFeeCollection(Assets.Sol, testContext),
  ]);

  // Test the affiliate withdrawal functionality
  const [broker, affiliateId, refundAddress] = await testFeeCollection(Assets.Btc, testContext);
  await testWithdrawCollectedAffiliateFees(broker, affiliateId, refundAddress, testContext.logger);
}

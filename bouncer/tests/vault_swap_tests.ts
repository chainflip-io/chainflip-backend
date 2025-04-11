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
import { executeVaultSwap, performVaultSwap } from '../shared/perform_swap';
import { prepareSwap } from '../shared/swapping';
import { getChainflipApi } from '../shared/utils/substrate';
import { getBalance } from '../shared/get_balance';
import { TestContext } from '../shared/utils/test_context';
import { Logger } from '../shared/utils/logger';

// Fee to use for the broker and affiliates
const commissionBps = 100;

async function testRefundVaultSwap(logger: Logger) {
  logger.info('Starting refund vault swap test...');

  const inputAsset = Assets.Btc;
  const destAsset = Assets.Usdc;
  const balanceObserveTimeout = 60;
  const depositAmount = defaultAssetAmounts(inputAsset);
  const destAddress = await newAddress('Usdc', 'BTC_VAULT_SWAP_REFUND' + Math.random() * 100);
  const refundAddress = await newAddress('Btc', 'BTC_VAULT_SWAP_REFUND' + Math.random() * 100);
  const foKParams = {
    retryDurationBlocks: 100,
    refundAddress,
    minPriceX128: '0',
  };

  logger.info('Sending vault swap...');

  await executeVaultSwap(
    logger,
    inputAsset,
    destAsset,
    destAddress,
    undefined,
    depositAmount,
    0, // boostFeeBps
    foKParams,
  );

  logger.info('Waiting for refund...');

  let btcBalance = false;

  for (let i = 0; i < balanceObserveTimeout; i++) {
    const refundAddressBalance = await getBalance(Assets.Btc, refundAddress);
    if (refundAddressBalance !== '0') {
      btcBalance = true;
      break;
    }
    await sleep(1000);
  }

  assert(btcBalance, `Vault swap refund failed 🙅‍♂️.`);

  logger.info('Refund vault swap completed ✅.');
}

async function testWithdrawCollectedAffiliateFees(
  logger: Logger,
  broker: KeyringPair,
  affiliateAccountId: string,
  withdrawAddress: string,
) {
  const chainflip = await getChainflipApi();

  const balanceObserveTimeout = 60;
  let success = false;

  logger.debug('Affiliate account ID:', affiliateAccountId);
  logger.debug('Withdraw address:', withdrawAddress);

  await chainflip.tx.swapping
    .affiliateWithdrawalRequest(affiliateAccountId)
    .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip));

  logger.info('Withdrawal request sent!');
  logger.debug('Waiting for balance change... Observing address:', withdrawAddress);

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
  let logger = testContext.logger;
  // Setup broker accounts. Different for each asset and specific to this test.
  const brokerUri = `//BROKER_VAULT_FEE_COLLECTION_${inputAsset}`;
  const broker = createStateChainKeypair(brokerUri);
  const refundAddress = await newAddress('Eth', 'BTC_VAULT_SWAP_REFUND' + Math.random() * 100);
  await Promise.all([setupBrokerAccount(logger, brokerUri)]);
  if (inputAsset === Assets.Btc) {
    await openPrivateBtcChannel(logger, brokerUri);
  }

  logger.debug('Registering affiliate');
  const event = await registerAffiliate(logger, brokerUri, refundAddress);

  const affiliateId = event.data.affiliateId as string;

  logger.debug('Broker:', broker.address);
  logger.debug('Affiliate:', affiliateId);

  // Setup
  const feeAsset = Assets.Usdc;
  const destAsset = inputAsset === feeAsset ? Assets.Flip : feeAsset;
  const depositAmount = defaultAssetAmounts(inputAsset);
  const { destAddress, tag } = await prepareSwap(
    logger,
    inputAsset,
    feeAsset,
    undefined, // addressType
    undefined, // messageMetadata
    'VaultSwapFeeTest',
    testContext.swapContext,
  );
  logger = logger.child({ tag });

  // Amounts before swap
  const earnedBrokerFeesBefore = await getEarnedBrokerFees(broker.address);
  const earnedAffiliateFeesBefore = await getEarnedBrokerFees(affiliateId);
  logger.debug('Earned broker fees before:', earnedBrokerFeesBefore);
  logger.debug('Earned affiliate fees before:', earnedAffiliateFeesBefore);

  // Do the vault swap
  await performVaultSwap(
    logger,
    inputAsset,
    destAsset,
    destAddress,
    undefined, // messageMetadata
    testContext.swapContext,
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

export async function testVaultSwap(testContext: TestContext) {
  await Promise.all([
    testFeeCollection(Assets.Eth, testContext),
    testFeeCollection(Assets.ArbEth, testContext),
    testFeeCollection(Assets.Sol, testContext),
  ]);

  // Test the affiliate withdrawal functionality
  const [broker, affiliateId, refundAddress] = await testFeeCollection(Assets.Btc, testContext);
  await testWithdrawCollectedAffiliateFees(testContext.logger, broker, affiliateId, refundAddress);
  await testRefundVaultSwap(testContext.logger);
}

import assert from 'assert';
import { InternalAsset as Asset } from '@chainflip/cli';
// eslint-disable-next-line no-restricted-imports
import type { KeyringPair } from '@polkadot/keyring/types';
import {
  Assets,
  createStateChainKeypair,
  defaultAssetAmounts,
  handleSubstrateError,
  newAssetAddress,
  sleep,
} from 'shared/utils';
import { getEarnedBrokerFees } from 'tests/broker_fee_collection';
import { buildAndSendInvalidBtcVaultSwap, registerAffiliate } from 'shared/btc_vault_swap';
import { setupBrokerAccount } from 'shared/setup_account';
import { executeVaultSwap, performVaultSwap } from 'shared/perform_swap';
import { prepareSwap } from 'shared/swapping';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { getBalance } from 'shared/get_balance';
import { TestContext } from 'shared/utils/test_context';
import { Logger } from 'shared/utils/logger';
import { ChainflipIO, newChainflipIO } from 'shared/utils/chainflip_io';

// Fee to use for the broker and affiliates
const commissionBps = 100;

async function testRefundVaultSwap(logger: Logger) {
  logger.info('Starting refund vault swap test...');

  const inputAsset = Assets.Btc;
  const destAsset = Assets.Usdc;
  const balanceObserveTimeout = 60;
  const depositAmount = defaultAssetAmounts(inputAsset);
  const destAddress = await newAssetAddress(destAsset);
  const refundAddress = await newAssetAddress(inputAsset);
  const foKParams = {
    retryDurationBlocks: 100,
    refundAddress,
    minPriceX128: '0',
  };

  logger.info('Sending vault swap...');

  await executeVaultSwap(
    logger,
    '//BROKER_1',
    inputAsset,
    destAsset,
    destAddress,
    undefined,
    depositAmount,
    0, // boostFeeBps
    foKParams,
  );

  logger.info(`Waiting for refund of ${inputAsset} to ${refundAddress}...`);

  let btcBalance = false;

  for (let i = 0; i < balanceObserveTimeout; i++) {
    const refundAddressBalance = await getBalance(inputAsset, refundAddress);
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

  const nonce = await chainflip.rpc.system.accountNextIndex(broker.address);
  await chainflip.tx.swapping
    .affiliateWithdrawalRequest(affiliateAccountId)
    .signAndSend(broker, { nonce }, handleSubstrateError(chainflip));

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

async function testFeeCollection<A = []>(
  parentcf: ChainflipIO<A>,
  inputAsset: Asset,
  testContext: TestContext,
): Promise<[KeyringPair, string, string]> {
  let cf = parentcf;
  // Setup broker accounts. Different for each asset and specific to this test.
  const brokerUri = `//BROKER_VAULT_FEE_COLLECTION_${inputAsset}`;
  const broker = createStateChainKeypair(brokerUri);
  const refundAddress = await newAssetAddress('Eth', 'BTC_VAULT_SWAP_REFUND' + Math.random() * 100);
  await setupBrokerAccount(cf.logger, brokerUri);
  cf.debug('Registering affiliate');
  const { affiliateId, shortId } = await registerAffiliate(cf.logger, brokerUri, refundAddress);

  cf.debug('Broker:', broker.address);
  cf.debug('Affiliate:', affiliateId);
  cf.debug('Short ID:', shortId);

  // Setup
  const feeAsset = Assets.Usdc;
  const destAsset = inputAsset === feeAsset ? Assets.Flip : feeAsset;
  const depositAmount = defaultAssetAmounts(inputAsset);
  const { destAddress, tag } = await prepareSwap(
    cf.logger,
    inputAsset,
    feeAsset,
    undefined, // addressType
    undefined, // messageMetadata
    'VaultSwapFeeTest',
    testContext.swapContext,
  );
  cf = cf.withChildLogger(tag);

  // Amounts before swap
  const earnedBrokerFeesBefore = await getEarnedBrokerFees(cf.logger, broker.address);
  const earnedAffiliateFeesBefore = await getEarnedBrokerFees(cf.logger, affiliateId);
  cf.debug('Earned broker fees before:', earnedBrokerFeesBefore);
  cf.debug('Earned affiliate fees before:', earnedAffiliateFeesBefore);

  // Do the vault swap
  await performVaultSwap(
    cf,
    brokerUri,
    inputAsset,
    destAsset,
    destAddress,
    undefined, // messageMetadata
    testContext.swapContext,
    depositAmount,
    0, // boostFeeBps
    undefined, // fillOrKillParams
    undefined, // dcaParams
    commissionBps,
    [{ accountAddress: affiliateId, commissionBps }],
  );

  // Check that both the broker and affiliate earned fees
  const earnedBrokerFeesAfter = await getEarnedBrokerFees(cf.logger, broker.address);
  const earnedAffiliateFeesAfter = await getEarnedBrokerFees(cf.logger, affiliateId);
  cf.debug('Earned broker fees after:', earnedBrokerFeesAfter);
  cf.debug('Earned affiliate fees after:', earnedAffiliateFeesAfter);
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

async function testInvalidBtcVaultSwap(logger: Logger) {
  logger.info('Starting invalid BTC vault swap test...');

  const inputAsset = Assets.Btc;
  const destAsset = Assets.Usdc;
  const depositAmount = defaultAssetAmounts(inputAsset);
  const destAddress = await newAssetAddress(destAsset);

  const txId = await buildAndSendInvalidBtcVaultSwap(
    logger,
    '//BROKER_1',
    Number(depositAmount),
    destAsset,
    destAddress,
    await newAssetAddress(inputAsset, 'BTC_VAULT_SWAP_REFUND'),
    Number(10),
  );

  logger.debug(`BTC vault swap txid is ${txId}, awaiting deposit finalised event...`);
  await observeEvent(logger, 'bitcoinIngressEgress:DepositFinalised', {
    test: (event) => event.data.action === 'Unrefundable',
    timeoutSeconds: 120,
    historicalCheckBlocks: 10,
  }).event;

  logger.info('Invalid BTC vault swap ingressed ✅.');
}

export async function testVaultSwap(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
  await cf.all([
    (subcf) => testFeeCollection(subcf, Assets.Eth, testContext),
    (subcf) => testFeeCollection(subcf, Assets.ArbEth, testContext),
    (subcf) => testFeeCollection(subcf, Assets.Sol, testContext),
  ]);

  // Test the affiliate withdrawal functionality
  const [broker, affiliateId, refundAddress] = await testFeeCollection(cf, Assets.Btc, testContext);
  await testWithdrawCollectedAffiliateFees(testContext.logger, broker, affiliateId, refundAddress);
  await testRefundVaultSwap(testContext.logger);
  await testInvalidBtcVaultSwap(testContext.logger);
}

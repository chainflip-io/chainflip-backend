import assert from 'assert';
import { InternalAsset as Asset } from '@chainflip/cli';
import { Assets, defaultAssetAmounts, newAssetAddress, sleep } from 'shared/utils';
import { getEarnedBrokerFees } from 'tests/broker_fee_collection';
import { buildAndSendInvalidBtcVaultSwap, registerAffiliate } from 'shared/btc_vault_swap';
import { AccountRole, setupAccount } from 'shared/setup_account';
import { executeVaultSwap, performVaultSwap } from 'shared/perform_swap';
import { prepareSwap } from 'shared/swapping';
import { getBalance } from 'shared/get_balance';
import { TestContext } from 'shared/utils/test_context';
import {
  ChainflipIO,
  FullAccount,
  fullAccountFromUri,
  newChainflipIO,
  WithBrokerAccount,
} from 'shared/utils/chainflip_io';
import { bitcoinIngressEgressDepositFinalised } from 'generated/events/bitcoinIngressEgress/depositFinalised';

// Fee to use for the broker and affiliates
const commissionBps = 100;

async function testRefundVaultSwap<A = []>(parentCf: ChainflipIO<A>) {
  const brokerUri = '//BROKER_1';
  const cf = parentCf.with({ account: fullAccountFromUri(brokerUri, 'Broker') });

  cf.info('Starting refund vault swap test...');

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

  cf.info('Sending vault swap...');
  await executeVaultSwap(
    cf.logger,
    brokerUri,
    inputAsset,
    destAsset,
    destAddress,
    undefined,
    depositAmount,
    0, // boostFeeBps
    foKParams,
  );

  cf.info(`Waiting for refund of ${inputAsset} to ${refundAddress}...`);

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

  cf.info('Refund vault swap completed ✅.');
}

// Note: if the collected fees are low, this function will fail with e.g.
// `ethereumIngressEgress.BelowEgressDustLimit: The amount is below the minimum egress amount.`
// In order to make bouncer less flaky we currently don't test this.
// eslint-disable-next-line @typescript-eslint/no-unused-vars
async function testWithdrawCollectedAffiliateFees<A extends WithBrokerAccount>(
  cf: ChainflipIO<A>,
  affiliateAccountId: string,
  withdrawAddress: string,
) {
  const balanceObserveTimeout = 60;
  let success = false;

  cf.debug('Affiliate account ID:', affiliateAccountId);
  cf.debug('Withdraw address:', withdrawAddress);

  await cf.submitExtrinsic({
    extrinsic: (api) => api.tx.swapping.affiliateWithdrawalRequest(affiliateAccountId),
  });

  cf.info('Withdrawal request sent!');
  cf.debug('Waiting for balance change... Observing address:', withdrawAddress);

  // Wait for balance change
  for (let i = 0; i < balanceObserveTimeout; i++) {
    if ((await getBalance(Assets.Usdc, withdrawAddress)) !== '0') {
      success = true;
      break;
    }
    await sleep(1000);
  }

  assert(success, `Withdrawal failed - No balance change detected within the timeout period 🙅‍♂️.`);
  cf.info('Withdrawal successful ✅.');
}

async function testFeeCollection<A = []>(
  parentCf: ChainflipIO<A>,
  inputAsset: Asset,
  testContext: TestContext,
): Promise<[FullAccount<'Broker'>, string, string]> {
  // Setup broker accounts. Different for each asset and specific to this test.
  const brokerUri: `//${string}` = `//BROKER_VAULT_FEE_COLLECTION_${inputAsset}`;
  const broker = await setupAccount(parentCf.logger, brokerUri, AccountRole.Broker);
  const brokerAccount = fullAccountFromUri(brokerUri, 'Broker');

  const cf = parentCf.with({ account: brokerAccount });

  const refundAddress = await newAssetAddress('Eth', 'BTC_VAULT_SWAP_REFUND' + Math.random() * 100);

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
    parentCf.logger,
    inputAsset,
    feeAsset,
    undefined, // addressType
    undefined, // messageMetadata
    'VaultSwapFeeTest',
    testContext.swapContext,
  );

  // Amounts before swap
  const earnedBrokerFeesBefore = await getEarnedBrokerFees(cf.logger, broker.address);
  const earnedAffiliateFeesBefore = await getEarnedBrokerFees(cf.logger, affiliateId);
  cf.debug('Earned broker fees before:', earnedBrokerFeesBefore);
  cf.debug('Earned affiliate fees before:', earnedAffiliateFeesBefore);

  // Do the vault swap
  const subcf = cf.withChildLogger(tag);
  await performVaultSwap(
    subcf,
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

  return Promise.resolve([brokerAccount, affiliateId, refundAddress]);
}

async function testInvalidBtcVaultSwap<A = []>(parentCf: ChainflipIO<A>) {
  const brokerUri = '//BROKER_1';
  const cf = parentCf.with({ account: fullAccountFromUri(brokerUri, 'Broker') });

  cf.info('Starting invalid BTC vault swap test...');

  const inputAsset = Assets.Btc;
  const destAsset = Assets.Usdc;
  const depositAmount = defaultAssetAmounts(inputAsset);
  const destAddress = await newAssetAddress(destAsset);

  const txId = await buildAndSendInvalidBtcVaultSwap(
    cf.logger,
    brokerUri,
    Number(depositAmount),
    destAsset,
    destAddress,
    await newAssetAddress(inputAsset, 'BTC_VAULT_SWAP_REFUND'),
    Number(10),
  );

  cf.debug(`BTC vault swap txid is ${txId}, awaiting deposit finalised event...`);

  await cf.stepUntilEvent(
    'BitcoinIngressEgress.DepositFinalised',
    bitcoinIngressEgressDepositFinalised.refine((event) => event.action.__kind === 'Unrefundable'),
  );

  cf.info('Invalid BTC vault swap ingressed ✅.');
}

export async function testVaultSwap(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
  await cf.all([
    (subcf) => testFeeCollection(subcf, Assets.Eth, testContext),
    (subcf) => testFeeCollection(subcf, Assets.ArbEth, testContext),
    (subcf) => testFeeCollection(subcf, Assets.Sol, testContext),
  ]);

  // NOTE: the following is currently disabled due to fee withdrawal being flaky.
  //
  // Test the affiliate withdrawal functionality
  // const [broker, affiliateId, refundAddress] = await testFeeCollection(cf, Assets.Btc, testContext);
  // await testWithdrawCollectedAffiliateFees(
  //   cf.with({ account: broker }),
  //   affiliateId,
  //   refundAddress,
  // );

  await testRefundVaultSwap(cf);
  await testInvalidBtcVaultSwap(cf);
}

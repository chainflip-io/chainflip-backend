import { Chains, InternalAsset } from '@chainflip/cli';
import {
  newAssetAddress,
  sleep,
  createStateChainKeypair,
  chainFromAsset,
  ingressEgressPalletForChain,
  observeBalanceIncrease,
  observeCcmReceived,
  observeFetch,
} from 'shared/utils';
import { observeEvent } from 'shared/utils/substrate';
import { requestNewSwap } from 'shared/perform_swap';
import { FillOrKillParamsX128 } from 'shared/new_swap';
import { TestContext } from 'shared/utils/test_context';
import { getBalance } from 'shared/get_balance';
import { send } from 'shared/send';
import { newCcmMetadata } from 'shared/swapping';
import { executeSolVaultSwap } from 'shared/sol_vault_swap';

const brokerUri = '//BROKER_1';

export async function testSol(
  testContext: TestContext,
  sourceAsset: InternalAsset,
  reportFunction: (txId: string) => Promise<void>,
  ccmRefund = false,
) {
  const logger = testContext.logger;
  logger.info(`Testing broker level screening for Sol ${sourceAsset}...`);

  const chain = chainFromAsset(sourceAsset);
  if (chain !== Chains.Solana) {
    // This should always be Sol
    throw new Error('Expected chain to be Solana');
  }
  const ingressEgressPallet = ingressEgressPalletForChain(chain);

  const destinationAddressForBtc = await newAssetAddress('Btc');

  logger.debug(`BTC destination address: ${destinationAddressForBtc}`);

  const solRefundAddress = await newAssetAddress('Sol', undefined, undefined, ccmRefund);
  const initialRefundAddressBalance = await getBalance(sourceAsset, solRefundAddress);

  const refundCcmMetadata = ccmRefund ? await newCcmMetadata(sourceAsset) : undefined;

  const refundParameters: FillOrKillParamsX128 = {
    retryDurationBlocks: 0,
    refundAddress: solRefundAddress,
    minPriceX128: '0',
    refundCcmMetadata,
  };

  const swapParams = await requestNewSwap(
    logger,
    sourceAsset,
    'Btc',
    destinationAddressForBtc,
    undefined,
    0,
    0,
    refundParameters,
  );

  logger.debug(`Sending ${sourceAsset} tx to reject...`);
  const result = await send(logger, sourceAsset, swapParams.depositAddress);
  const txHash = result.transaction.signatures[0] as string;
  logger.debug(`Sent ${sourceAsset} tx, hash is ${txHash}`);

  await reportFunction(`ActualSignature(S(${txHash}))`);
  logger.debug(`Marked ${sourceAsset} ${txHash} for rejection. Awaiting refund.`);

  await observeEvent(logger, `${ingressEgressPallet}:TransactionRejectedByBroker`).event;

  const ccmEventEmitted = refundParameters.refundCcmMetadata
    ? observeCcmReceived(
        sourceAsset,
        sourceAsset,
        refundParameters.refundAddress,
        refundParameters.refundCcmMetadata,
      )
    : Promise.resolve();

  await Promise.all([
    observeBalanceIncrease(logger, sourceAsset, solRefundAddress, initialRefundAddressBalance),
    ccmEventEmitted,
    observeFetch(sourceAsset, swapParams.depositAddress),
  ]);

  logger.info(`Marked ${sourceAsset} transaction was rejected and refunded 👍.`);
}

export async function testSolVaultSwap(
  testContext: TestContext,
  sourceAsset: InternalAsset,
  reportFunction: (txId: string) => Promise<void>,
) {
  const logger = testContext.logger;

  const chain = chainFromAsset(sourceAsset);
  if (chain !== Chains.Solana) {
    // This should always be Sol
    throw new Error('Expected chain to be Solana');
  }

  logger.info(`Testing broker level screening for ${chain} ${sourceAsset} vault swap...`);
  const MAX_RETRIES = 120;

  const destinationAddressForBtc = await newAssetAddress('Btc');
  const solanaRefundAddress = await newAssetAddress('Sol');

  logger.debug(`Refund address for ${sourceAsset} is ${solanaRefundAddress}...`);

  logger.debug(`Sending ${sourceAsset} (vault swap) tx to reject...`);

  const receipt = await executeSolVaultSwap(
    logger,
    sourceAsset,
    'Btc',
    destinationAddressForBtc,
    {
      account: createStateChainKeypair(brokerUri).address,
      commissionBps: 0.0,
    },
    undefined,
    undefined,
    undefined,
    {
      retryDurationBlocks: 0,
      refundAddress: solanaRefundAddress,
      minPriceX128: '0x0',
    },
    undefined,
    [],
  );
  const txHash = receipt.txHash;
  logger.debug(`Sent ${sourceAsset} (vault swap) tx...`);

  await reportFunction(`ActualSignature(S(${txHash}))`);
  logger.debug(`Marked ${sourceAsset} (vault swap) ${txHash} for rejection. Awaiting refund.`);

  // Currently this event cannot be decoded correctly, so we don't wait for it,
  // just wait for the funds to arrive at the refund address
  // await observeEvent(`${ingressEgressPallet}:TransactionRejectedByBroker`).event;

  let receivedRefund = false;
  for (let i = 0; i < MAX_RETRIES; i++) {
    const refundBalance = await getBalance(sourceAsset, solanaRefundAddress);
    if (refundBalance !== '0') {
      receivedRefund = true;
      break;
    }
    await sleep(6000);
  }

  if (!receivedRefund) {
    throw new Error(
      `Didn't receive funds refund to address ${solanaRefundAddress} within timeout!`,
    );
  }
  logger.info(`Marked ${sourceAsset} vault swap was rejected and refunded 👍.`);
}

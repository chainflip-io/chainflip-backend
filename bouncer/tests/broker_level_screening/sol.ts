import { InternalAsset } from '@chainflip/cli';
import {
  newAssetAddress,
  sleep,
  createStateChainKeypair,
  chainFromAsset,
  ingressEgressPalletForChain,
  observeBalanceIncrease,
  observeCcmReceived,
  observeFetch,
  Chains,
} from 'shared/utils';
import { observeEvent } from 'shared/utils/substrate';
import { requestNewSwap } from 'shared/perform_swap';
import { FillOrKillParamsX128 } from 'shared/new_swap';
import { getBalance } from 'shared/get_balance';
import { send } from 'shared/send';
import { newCcmMetadata } from 'shared/swapping';
import { executeSolVaultSwap } from 'shared/sol_vault_swap';
import { ChainflipIO } from 'shared/utils/chainflip_io';

const brokerUri = '//BROKER_1';

export async function testSol<A = []>(
  cf: ChainflipIO<A>,
  sourceAsset: InternalAsset,
  reportFunction: (txId: string) => Promise<void>,
  ccmRefund = false,
) {
  cf.info(`Testing broker level screening for Sol ${sourceAsset}...`);

  const chain = chainFromAsset(sourceAsset);
  if (chain !== Chains.Solana) {
    // This should always be Sol
    throw new Error('Expected chain to be Solana');
  }
  const ingressEgressPallet = ingressEgressPalletForChain(chain);

  const destinationAddressForBtc = await newAssetAddress('Btc');

  cf.debug(`BTC destination address: ${destinationAddressForBtc}`);

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
    cf,
    sourceAsset,
    'Btc',
    destinationAddressForBtc,
    undefined,
    0,
    0,
    refundParameters,
  );

  cf.debug(`Sending ${sourceAsset} tx to reject...`);
  const result = await send(cf.logger, sourceAsset, swapParams.depositAddress);
  const txHash = result.transaction.signatures[0] as string;
  cf.debug(`Sent ${sourceAsset} tx, hash is ${txHash}`);

  await reportFunction(txHash);
  cf.debug(`Marked ${sourceAsset} ${txHash} for rejection. Awaiting refund.`);

  await observeEvent(cf.logger, `${ingressEgressPallet}:TransactionRejectedByBroker`).event;

  const ccmEventEmitted = refundParameters.refundCcmMetadata
    ? observeCcmReceived(
        sourceAsset,
        sourceAsset,
        refundParameters.refundAddress,
        refundParameters.refundCcmMetadata,
      )
    : Promise.resolve();

  await Promise.all([
    observeBalanceIncrease(
      cf.logger,
      sourceAsset,
      solRefundAddress,
      initialRefundAddressBalance,
      360,
    ),
    ccmEventEmitted,
    observeFetch(sourceAsset, swapParams.depositAddress),
  ]);

  cf.info(`Marked ${sourceAsset} transaction was rejected and refunded 👍.`);
}

export async function testSolVaultSwap<A = []>(
  cf: ChainflipIO<A>,
  sourceAsset: InternalAsset,
  reportFunction: (txId: string) => Promise<void>,
) {
  const chain = chainFromAsset(sourceAsset);
  if (chain !== Chains.Solana) {
    // This should always be Sol
    throw new Error('Expected chain to be Solana');
  }

  cf.info(`Testing broker level screening for ${chain} ${sourceAsset} vault swap...`);
  const MAX_RETRIES = 120;

  const destinationAddressForBtc = await newAssetAddress('Btc');
  const solanaRefundAddress = await newAssetAddress('Sol');

  cf.debug(`Refund address for ${sourceAsset} is ${solanaRefundAddress}...`);

  cf.debug(`Sending ${sourceAsset} (vault swap) tx to reject...`);

  const receipt = await executeSolVaultSwap(
    cf.logger,
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
      minPriceX128: '0',
    },
    undefined,
    [],
  );
  const txHash = receipt.txHash;
  cf.debug(`Sent ${sourceAsset} (vault swap) tx...`);

  await reportFunction(txHash);
  cf.debug(`Marked ${sourceAsset} (vault swap) ${txHash} for rejection. Awaiting refund.`);

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
  cf.info(`Marked ${sourceAsset} vault swap was rejected and refunded 👍.`);
}

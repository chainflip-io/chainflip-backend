import {
  newAssetAddress,
  sleep,
  chainFromAsset,
  observeBalanceIncrease,
  observeCcmReceived,
  observeFetch,
  Chains,
  decodeSolAddress,
  Asset,
} from 'shared/utils';
import { requestNewSwap } from 'shared/perform_swap';
import { FillOrKillParamsX128 } from 'shared/new_swap';
import { getBalance } from 'shared/get_balance';
import { send } from 'shared/send';
import { newCcmMetadata } from 'shared/swapping';
import { executeSolVaultSwap } from 'shared/sol_vault_swap';
import { ChainflipIO, fullAccountFromUri } from 'shared/utils/chainflip_io';
import { solanaIngressEgressTransactionRejectedByBroker } from 'generated/events/solanaIngressEgress/transactionRejectedByBroker';
import { solanaIngressEgressDepositFinalised } from 'generated/events/solanaIngressEgress/depositFinalised';

export async function testSol<A = []>(
  parentCf: ChainflipIO<A>,
  sourceAsset: Asset,
  reportFunction: (txId: string) => Promise<void>,
  ccmRefund = false,
) {
  const cf = parentCf.withChildLogger(`${sourceAsset}_BrokerLevelScreening_testSol`);
  cf.info(`Testing broker level screening for Sol ${sourceAsset}...`);

  const chain = chainFromAsset(sourceAsset);
  if (chain !== Chains.Solana) {
    // This should always be Sol
    throw new Error('Expected chain to be Solana');
  }
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

  const ccmEventEmitted = refundParameters.refundCcmMetadata
    ? observeCcmReceived(
        sourceAsset,
        sourceAsset,
        refundParameters.refundAddress,
        refundParameters.refundCcmMetadata,
      )
    : Promise.resolve();

  cf.debug(`Sending ${sourceAsset} tx to reject...`);
  const result = await send(cf.logger, sourceAsset, swapParams.depositAddress);
  const txHash = result.transaction.signatures[0] as string;
  cf.debug(`Sent ${sourceAsset} tx, hash is ${txHash}`);

  await reportFunction(txHash);
  cf.debug(`Marked ${sourceAsset} ${txHash} for rejection. Awaiting refund.`);

  const resultEvent = await cf.stepUntilOneEventOf({
    transactionRejected: {
      name: 'SolanaIngressEgress.TransactionRejectedByBroker',
      schema: solanaIngressEgressTransactionRejectedByBroker.refine(
        (event) =>
          event.txId.__kind === 'Channel' &&
          event.txId.value === decodeSolAddress(swapParams.depositAddress),
      ),
    },
    depositFinalized: {
      name: 'SolanaIngressEgress.DepositFinalised',
      schema: solanaIngressEgressDepositFinalised.refine(
        (event) =>
          event.depositDetails.__kind === 'Channel' &&
          event.depositDetails.value === decodeSolAddress(swapParams.depositAddress),
      ),
    },
  });

  if (resultEvent.key === 'depositFinalized') {
    throw new Error(
      `Failed to reject Solana tx ${txHash}. The transaction was ingressed instead of being rejected.
       It might be because the deposit monitor was late in reporting the tx and the transaction ended up being swapped instead`,
    );
  }

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
  parentCf: ChainflipIO<A>,
  sourceAsset: Asset,
  reportFunction: (txId: string) => Promise<void>,
) {
  const cf = parentCf.with({ account: fullAccountFromUri('//BROKER_1', 'Broker') });

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
    cf,
    sourceAsset,
    'Btc',
    destinationAddressForBtc,
    0.0,
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
  // await cf.stepUntilEvent(
  //   'SolanaIngressEgress.TransactionRejectedByBroker',
  //   solanaIngressEgressTransactionRejectedByBroker.refine(
  //     (event) =>
  //       event.txId.__kind === 'VaultSwapAccount' &&
  //       event.txId.value[0] === decodeSolAddress(receipt.accountAddress.toString()), // fix decoding
  //   ),
  // );

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

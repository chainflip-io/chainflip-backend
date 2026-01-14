import { InternalAsset as Asset } from '@chainflip/cli';
import { DcaParams, newSwap, FillOrKillParamsX128, CcmDepositMetadata } from 'shared/new_swap';
import { send, sendViaCfTester } from 'shared/send';
import { getBalance } from 'shared/get_balance';
import {
  observeBalanceIncrease,
  observeCcmReceived,
  observeSwapEvents,
  observeBroadcastSuccess,
  observeFetch,
  chainFromAsset,
  observeSwapRequested,
  SwapRequestType,
  evmChains,
  createEvmWalletAndFund,
  getSolWhaleKeyPair,
  decodeSolAddress,
  VaultSwapParams,
  TransactionOriginId,
  TransactionOrigin,
  defaultAssetAmounts,
  newAssetAddress,
  getContractAddress,
  createStateChainKeypair,
} from 'shared/utils';
import { SwapContext, SwapStatus } from 'shared/utils/swap_context';
import { getChainflipApi } from 'shared/utils/substrate';
import { executeEvmVaultSwap } from 'shared/evm_vault_swap';
import { executeSolVaultSwap } from 'shared/sol_vault_swap';
import { buildAndSendBtcVaultSwap } from 'shared/btc_vault_swap';
import { Logger, throwError } from 'shared/utils/logger';
import { swappingSwapDepositAddressReady } from 'generated/events/swapping/swapDepositAddressReady';
import { swappingSwapRequestCompleted } from 'generated/events/swapping/swapRequestCompleted';
import { swappingSwapEgressScheduled } from 'generated/events/swapping/swapEgressScheduled';
import { ChainflipIO } from 'shared/utils/chainflip_io';

export type SwapParams = {
  sourceAsset: Asset;
  destAsset: Asset;
  depositAddress: string;
  destAddress: string;
  channelId: number;
};

export async function requestNewSwap<A = []>(
  cf: ChainflipIO<A>,
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  messageMetadata?: CcmDepositMetadata,
  brokerCommissionBps?: number,
  boostFeeBps = 0,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
): Promise<SwapParams> {
  cf.debug(
    `Requesting swap with sourceAsset ${sourceAsset}, destinationAsset ${destAsset}, destinationAddress ${destAddress} and metadata ${JSON.stringify(messageMetadata)}`,
  );
  await newSwap(
    cf,
    sourceAsset,
    destAsset,
    destAddress,
    messageMetadata,
    brokerCommissionBps,
    boostFeeBps,
    fillOrKillParams,
    dcaParams,
  );
  const addressReady = await cf.expectEvent(
    'Swapping.SwapDepositAddressReady',
    swappingSwapDepositAddressReady.refine((event) => {
      const eventMatches =
        event.destinationAddress.address.toLowerCase() === destAddress.toLowerCase() &&
        event.destinationAsset === destAsset &&
        event.sourceAsset === sourceAsset;

      const ccmMetadataMatches = messageMetadata
        ? event.channelMetadata !== undefined &&
          event.channelMetadata?.message ===
            (messageMetadata.message === '0x' ? '' : messageMetadata.message) &&
          event.channelMetadata.gasBudget === BigInt(messageMetadata.gasBudget)
        : event.channelMetadata === undefined;

      return eventMatches && ccmMetadataMatches;
    }),
  );

  const depositAddress = addressReady.depositAddress.address;
  const channelDestAddress = addressReady.destinationAddress.address;
  const channelId = Number(addressReady.channelId);

  cf.debug(
    `Deposit address: ${depositAddress}, Destination address: ${channelDestAddress}, Channel ID: ${channelId}`,
  );

  return {
    sourceAsset,
    destAsset,
    depositAddress,
    destAddress,
    channelId,
  };
}

export enum SenderType {
  Address,
  Vault,
}

// Note: if using the swap context, the logger must contain the tag
export async function doPerformSwap<A = []>(
  cf: ChainflipIO<A>,
  { sourceAsset, destAsset, destAddress, depositAddress, channelId }: SwapParams,
  messageMetadata?: CcmDepositMetadata,
  senderType = SenderType.Address,
  amount?: string,
  swapContext?: SwapContext,
) {
  const oldBalance = await getBalance(destAsset, destAddress);

  cf.trace(`Old balance: ${oldBalance}`);

  const swapRequestedHandle = observeSwapRequested(
    cf,
    sourceAsset,
    destAsset,
    { type: TransactionOrigin.DepositChannel, channelId },
    SwapRequestType.Regular,
  );

  const ccmEventEmitted = messageMetadata
    ? observeCcmReceived(sourceAsset, destAsset, destAddress, messageMetadata)
    : Promise.resolve();

  const txId = await (senderType === SenderType.Address
    ? send(cf.logger, sourceAsset, depositAddress, amount)
    : sendViaCfTester(cf.logger, sourceAsset, depositAddress));

  cf.debug(`Funded the address with tx ${txId}`);

  swapContext?.updateStatus(cf.logger, SwapStatus.Funded);

  const swapRequestId = (await swapRequestedHandle).swapRequestId;

  swapContext?.updateStatus(cf.logger, SwapStatus.SwapScheduled);

  cf.debug(`Swap requested with ID: ${swapRequestId}`);

  await cf.stepUntilEvent(
    'Swapping.SwapRequestCompleted',
    swappingSwapRequestCompleted.refine((event) => event.swapRequestId === swapRequestId),
  );

  swapContext?.updateStatus(cf.logger, SwapStatus.SwapCompleted);

  cf.debug(`Swap Request Completed. Waiting for egress.`);

  const { egressId, amount: egressAmount } = await cf.stepUntilEvent(
    'Swapping.SwapEgressScheduled',
    swappingSwapEgressScheduled.refine((event) => event.swapRequestId === swapRequestId),
  );

  swapContext?.updateStatus(cf.logger, SwapStatus.EgressScheduled);

  cf.debug(
    `Egress ID: ${egressId}, Egress amount: ${egressAmount}. Waiting for balance to increase.`,
  );

  try {
    const [newBalance] = await Promise.all([
      observeBalanceIncrease(cf.logger, destAsset, destAddress, oldBalance),
      ccmEventEmitted,
    ]);

    const chain = chainFromAsset(sourceAsset);
    if (chain !== 'Bitcoin' && chain !== 'Polkadot' && chain !== 'Assethub') {
      cf.debug(`Waiting deposit fetch ${depositAddress}`);
      await observeFetch(sourceAsset, depositAddress);
    }

    cf.debug(`Swap success! New balance: ${newBalance}!`);
    swapContext?.updateStatus(cf.logger, SwapStatus.Success);
  } catch (err) {
    swapContext?.updateStatus(cf.logger, SwapStatus.Failure);
    throwError(cf.logger, new Error(`$${err}`));
  }
}

// Note: if using the swap context, the logger must contain the tag
export async function performSwap<A = []>(
  cf: ChainflipIO<A>,
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  messageMetadata?: CcmDepositMetadata,
  senderType = SenderType.Address,
  amount?: string,
  brokerCommissionBps?: number,
  swapContext?: SwapContext,
) {
  cf.trace(
    `The args are: ${sourceAsset} ${destAsset} ${destAddress} ${
      messageMetadata
        ? messageMetadata.message.substring(0, 6) +
          '...' +
          messageMetadata.message.substring(messageMetadata.message.length - 4)
        : ''
    }`,
  );

  const swapParams = await requestNewSwap(
    cf,
    sourceAsset,
    destAsset,
    destAddress,
    messageMetadata,
    brokerCommissionBps,
  );

  await doPerformSwap(cf, swapParams, messageMetadata, senderType, amount, swapContext);

  return swapParams;
}

// function to create a swap and track it until we detect the corresponding broadcast success
export async function performAndTrackSwap<A = []>(
  cf: ChainflipIO<A>,
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  amount?: string,
) {
  await using chainflipApi = await getChainflipApi();

  const swapParams = await requestNewSwap(cf, sourceAsset, destAsset, destAddress);

  await send(cf.logger, sourceAsset, swapParams.depositAddress, amount);
  cf.debug(`Funds sent, waiting for the deposit to be witnessed..`);

  // SwapScheduled, SwapExecuted, SwapEgressScheduled, BatchBroadcastRequested
  const broadcastId = await observeSwapEvents(cf.logger, swapParams, chainflipApi);

  if (broadcastId) {
    await observeBroadcastSuccess(cf.logger, broadcastId);
  } else {
    throwError(cf.logger, new Error(`Failed to retrieve broadcastId!`));
  }
  cf.debug(`Broadcast executed successfully, swap is complete!`);
}

export async function executeVaultSwap(
  logger: Logger,
  brokerUri: string,
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  messageMetadata?: CcmDepositMetadata,
  amount?: string,
  boostFeeBps?: number,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
  brokerFee: number = 1,
  affiliateFees: {
    accountAddress: string;
    commissionBps: number;
  }[] = [],
) {
  let sourceAddress: string;
  let transactionId: TransactionOriginId;

  const srcChain = chainFromAsset(sourceAsset);

  if (evmChains.includes(srcChain)) {
    logger.debug('Executing EVM vault swap');
    // Generate a new wallet for each vault swap to prevent nonce issues when running in parallel
    // with other swaps via deposit channels.
    const wallet = await createEvmWalletAndFund(logger, sourceAsset);
    sourceAddress = wallet.address.toLowerCase();

    // To uniquely identify the VaultSwap, we need to use the TX hash. This is only known
    // after sending the transaction, so we send it first and observe the events afterwards.
    // There are still multiple blocks of safety margin inbetween before the event is emitted
    const txHash = await executeEvmVaultSwap(
      logger,
      brokerUri,
      sourceAsset,
      destAsset,
      destAddress,
      brokerFee,
      messageMetadata,
      amount,
      boostFeeBps,
      fillOrKillParams,
      dcaParams,
      wallet,
      affiliateFees,
    );
    transactionId = { type: TransactionOrigin.VaultSwapEvm, txHash };
    sourceAddress = wallet.address.toLowerCase();
  } else if (srcChain === 'Bitcoin') {
    logger.debug('Executing BTC vault swap');
    const txId = await buildAndSendBtcVaultSwap(
      logger,
      brokerUri,
      Number(amount ?? defaultAssetAmounts(sourceAsset)),
      destAsset,
      destAddress,
      fillOrKillParams === undefined
        ? await newAssetAddress('Btc', 'BTC_VAULT_SWAP_REFUND')
        : fillOrKillParams.refundAddress,
      brokerFee,
      affiliateFees.map((f) => ({ account: f.accountAddress, bps: f.commissionBps })),
    );
    transactionId = { type: TransactionOrigin.VaultSwapBitcoin, txId };
    // Unused for now
    sourceAddress = '';
  } else {
    logger.debug('Executing Solana vault swap');
    const { slot, accountAddress } = await executeSolVaultSwap(
      logger,
      sourceAsset,
      destAsset,
      destAddress,
      {
        account: createStateChainKeypair(brokerUri).address,
        commissionBps: brokerFee,
      },
      messageMetadata,
      undefined,
      boostFeeBps,
      fillOrKillParams,
      dcaParams,
      affiliateFees.map((f) => ({ account: f.accountAddress, bps: f.commissionBps })),
    );
    transactionId = {
      type: TransactionOrigin.VaultSwapSolana,
      addressAndSlot: [decodeSolAddress(accountAddress.toBase58()), slot],
    };
    sourceAddress = decodeSolAddress(getSolWhaleKeyPair().publicKey.toBase58());
  }

  logger.debug(
    `vault swap sent on ${srcChain} with transactionId ${JSON.stringify(transactionId)} and source address ${sourceAddress}`,
  );

  return { transactionId, sourceAddress };
}

export async function performVaultSwap<A = []>(
  cf: ChainflipIO<A>,
  brokerUri: string,
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  messageMetadata?: CcmDepositMetadata,
  swapContext?: SwapContext,
  amount?: string,
  boostFeeBps?: number,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
  brokerFee?: number,
  affiliateFees: {
    accountAddress: string;
    commissionBps: number;
  }[] = [],
): Promise<VaultSwapParams> {
  const oldBalance = await getBalance(destAsset, destAddress);

  cf.debug(`Old balance: ${oldBalance}`);
  cf.debug(
    `Executing (${sourceAsset}) vault swap to(${destAsset}) ${destAddress}. Current balance: ${oldBalance}`,
  );

  try {
    const { transactionId, sourceAddress } = await executeVaultSwap(
      cf.logger,
      brokerUri,
      sourceAsset,
      destAsset,
      destAddress,
      messageMetadata,
      amount,
      boostFeeBps,
      fillOrKillParams,
      dcaParams,
      brokerFee,
      affiliateFees,
    );
    swapContext?.updateStatus(cf.logger, SwapStatus.VaultSwapInitiated);

    await observeSwapRequested(cf, sourceAsset, destAsset, transactionId, SwapRequestType.Regular);

    swapContext?.updateStatus(cf.logger, SwapStatus.VaultSwapScheduled);

    const [newBalance] = await cf.all([
      (subcf) => observeBalanceIncrease(subcf, destAsset, destAddress, oldBalance),
      async (subcf) => {
        if (messageMetadata) {
          subcf.debug(`Waiting for ccmEvent`);
          const result = await observeCcmReceived(
            sourceAsset,
            destAsset,
            destAddress,
            messageMetadata,
            sourceAddress,
          );
          subcf.debug(`Found ccmEvent!`);
          return result;
        } else {
          subcf.debug(`No message metadata, so not waiting for a ccm event.`);
          return;
        }
      },
    ]);
    cf.debug(`Swap success! New balance: ${newBalance}!`);

    if (sourceAsset === 'Sol') {
      // Native Vault swaps are fetched proactively. SPL-tokens don't need a fetch.
      const swapEndpointNativeVaultAddress = getContractAddress(
        'Solana',
        'SWAP_ENDPOINT_NATIVE_VAULT_ACCOUNT',
      );
      cf.debug(
        `Waiting for Swap Endpoint Native Vault Swap Fetch ${swapEndpointNativeVaultAddress}`,
      );
      await observeFetch(sourceAsset, swapEndpointNativeVaultAddress);
    }
    swapContext?.updateStatus(cf.logger, SwapStatus.Success);
    return {
      sourceAsset,
      destAsset,
      destAddress,
      transactionId,
    };
  } catch (err) {
    swapContext?.updateStatus(cf.logger, SwapStatus.Failure);
    if (err instanceof Error) {
      cf.debug(err.stack ?? '');
    }
    return throwError(cf.logger, new Error(`${err}`));
  }
}

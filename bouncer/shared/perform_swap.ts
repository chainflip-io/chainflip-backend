import { HDNodeWallet } from 'ethers';
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
  decodeDispatchError,
  Asset,
} from 'shared/utils';
import { SwapContext, SwapStatus } from 'shared/utils/swap_context';
import { getChainflipApi } from 'shared/utils/substrate';
import { executeEvmVaultSwap } from 'shared/evm_vault_swap';
import { executeSolVaultSwap } from 'shared/sol_vault_swap';
import { buildAndSendBtcVaultSwap } from 'shared/btc_vault_swap';
import { throwError } from 'shared/utils/logger';
import { swappingSwapDepositAddressReady } from 'generated/events/swapping/swapDepositAddressReady';
import { swappingSwapRequestCompleted } from 'generated/events/swapping/swapRequestCompleted';
import { swappingSwapEgressScheduled } from 'generated/events/swapping/swapEgressScheduled';
import { ChainflipIO, WithBrokerAccount } from 'shared/utils/chainflip_io';
import { swappingSwapEgressIgnored } from 'generated/events/swapping/swapEgressIgnored';

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
        event.destinationAddress.chain === chainFromAsset(destAsset) &&
        event.destinationAsset === destAsset &&
        event.sourceAsset === sourceAsset;

      const ccmMetadataMatches = messageMetadata
        ? event.channelMetadata !== undefined &&
          event.channelMetadata?.message ===
            (messageMetadata.message === '0x' ? '' : messageMetadata.message) &&
          event.channelMetadata.gasBudget === BigInt(messageMetadata.gasBudget)
        : event.channelMetadata === undefined;

      const dcaParamsMatches = dcaParams
        ? event.dcaParameters !== undefined &&
          event.dcaParameters?.numberOfChunks === dcaParams.numberOfChunks &&
          event.dcaParameters?.chunkInterval === dcaParams.chunkIntervalBlocks
        : event.dcaParameters === undefined;

      return eventMatches && ccmMetadataMatches && dcaParamsMatches;
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

async function waitForEgressScheduled<A = []>(
  cf: ChainflipIO<A>,
  swapRequestId: bigint,
  swapContext?: SwapContext,
): Promise<void> {
  const resultEvent = await cf.stepUntilOneEventOf({
    egressScheduled: {
      name: 'Swapping.SwapEgressScheduled',
      schema: swappingSwapEgressScheduled.refine((event) => event.swapRequestId === swapRequestId),
    },
    egressIgnored: {
      name: 'Swapping.SwapEgressIgnored',
      schema: swappingSwapEgressIgnored.refine((event) => event.swapRequestId === swapRequestId),
    },
  });

  if (resultEvent.key === 'egressIgnored') {
    const reason = decodeDispatchError(resultEvent.data.reason, await getChainflipApi());
    throwError(cf.logger, new Error(`Swap Egress was ignored reason: ${reason}`));
  } else {
    swapContext?.updateStatus(cf.logger, SwapStatus.EgressScheduled);
    cf.debug(`Egress ID: ${resultEvent.data.egressId}, Egress amount: ${resultEvent.data.amount}.`);
  }
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

  const ccmEventEmitted = messageMetadata
    ? observeCcmReceived(sourceAsset, destAsset, destAddress, messageMetadata)
    : Promise.resolve();

  const txId = await (senderType === SenderType.Address
    ? send(cf.logger, sourceAsset, depositAddress, amount)
    : sendViaCfTester(cf.logger, sourceAsset, depositAddress));

  cf.debug(`Funded the address with tx ${txId}`);
  swapContext?.updateStatus(cf.logger, SwapStatus.Funded);

  const swapRequestId = (
    await observeSwapRequested(
      cf,
      sourceAsset,
      destAsset,
      { type: TransactionOrigin.DepositChannel, channelId },
      SwapRequestType.Regular,
    )
  ).swapRequestId;

  swapContext?.updateStatus(cf.logger, SwapStatus.SwapScheduled);
  cf.debug(`Swap requested with ID: ${swapRequestId}`);

  await cf.stepUntilEvent(
    'Swapping.SwapRequestCompleted',
    swappingSwapRequestCompleted.refine((event) => event.swapRequestId === swapRequestId),
  );

  swapContext?.updateStatus(cf.logger, SwapStatus.SwapCompleted);
  cf.debug(
    `Swap Request Completed. Waiting for egress scheduled event, balance increase and CCM emitted (if CCM swap).`,
  );

  await waitForEgressScheduled(cf, swapRequestId, swapContext);

  try {
    const [newBalance] = await Promise.all([
      observeBalanceIncrease(cf.logger, destAsset, destAddress, oldBalance),
      ccmEventEmitted,
    ]);

    const chain = chainFromAsset(sourceAsset);
    if (chain !== 'Bitcoin' && chain !== 'Assethub') {
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

export type VaultSwapSource =
  | { chain: 'Evm'; wallet: HDNodeWallet; sourceAddress: string }
  | { chain: 'Bitcoin'; sourceAddress: string }
  | { chain: 'Solana'; sourceAddress: string };

export async function prepareVaultSwapSource<A = []>(
  cf: ChainflipIO<A>,
  sourceAsset: Asset,
  amount?: string,
): Promise<VaultSwapSource> {
  const srcChain = chainFromAsset(sourceAsset);
  let vaultSwapSource: VaultSwapSource;

  if (evmChains.includes(srcChain)) {
    // Generate a new wallet for each vault swap to prevent nonce issues when running in parallel
    // with other swaps via deposit channels.
    const wallet = await createEvmWalletAndFund(cf.logger, sourceAsset, amount);
    vaultSwapSource = { chain: 'Evm', wallet, sourceAddress: wallet.address.toLowerCase() };
  } else if (srcChain === 'Bitcoin') {
    // Unused for now
    vaultSwapSource = { chain: 'Bitcoin', sourceAddress: '' };
  } else if (srcChain === 'Solana') {
    vaultSwapSource = {
      chain: 'Solana',
      sourceAddress: decodeSolAddress(getSolWhaleKeyPair().publicKey.toBase58()),
    };
  } else {
    throwError(cf.logger, new Error('Unsupported vault swap source chain'));
  }

  return vaultSwapSource;
}

export async function executeVaultSwap<A extends WithBrokerAccount>(
  cf: ChainflipIO<A>,
  vaultSwapSource: VaultSwapSource,
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
  let transactionId: TransactionOriginId;

  const srcChain = chainFromAsset(sourceAsset);

  if (vaultSwapSource.chain === 'Evm') {
    cf.trace('Executing EVM vault swap');

    // To uniquely identify the VaultSwap, we need to use the TX hash. This is only known
    // after sending the transaction, so we send it first and observe the events afterwards.
    // There are still multiple blocks of safety margin inbetween before the event is emitted
    const txHash = await executeEvmVaultSwap(
      cf,
      sourceAsset,
      destAsset,
      destAddress,
      brokerFee,
      messageMetadata,
      amount,
      boostFeeBps,
      fillOrKillParams,
      dcaParams,
      vaultSwapSource.wallet,
      affiliateFees,
    );
    transactionId = { type: TransactionOrigin.VaultSwapEvm, txHash };
  } else if (vaultSwapSource.chain === 'Bitcoin') {
    cf.trace('Executing BTC vault swap');
    const txId = await buildAndSendBtcVaultSwap(
      cf,
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
  } else if (vaultSwapSource.chain === 'Solana') {
    cf.trace('Executing Solana vault swap');
    const { slot, accountAddress } = await executeSolVaultSwap(
      cf,
      sourceAsset,
      destAsset,
      destAddress,
      brokerFee,
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
  } else {
    throwError(cf.logger, new Error('Unsupported vault swap source chain'));
  }

  cf.debug(
    `vault swap sent on ${srcChain} with transactionId ${JSON.stringify(transactionId)} and source address ${vaultSwapSource.sourceAddress}`,
  );

  return { transactionId, sourceAddress: vaultSwapSource.sourceAddress };
}

export async function performVaultSwap<A extends WithBrokerAccount>(
  cf: ChainflipIO<A>,
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

  cf.trace(`Old balance: ${oldBalance}`);
  cf.trace(
    `Executing (${sourceAsset}) vault swap to(${destAsset}) ${destAddress}. Current balance: ${oldBalance}`,
  );

  try {
    const vaultSwapSource = await prepareVaultSwapSource(cf, sourceAsset, amount);

    // Start observing ccmEventEmitted before initiating the vault swap
    const ccmEventEmitted = messageMetadata
      ? observeCcmReceived(
          sourceAsset,
          destAsset,
          destAddress,
          messageMetadata,
          vaultSwapSource.sourceAddress,
        )
      : Promise.resolve();

    const { transactionId } = await executeVaultSwap(
      cf,
      vaultSwapSource,
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

    const swapRequestedEvent = await observeSwapRequested(
      cf,
      sourceAsset,
      destAsset,
      transactionId,
      SwapRequestType.Regular,
    );
    cf.debug(
      `Observed Swapping.SwapRequested event with swapRequestId ${swapRequestedEvent.swapRequestId}`,
    );
    swapContext?.updateStatus(cf.logger, SwapStatus.VaultSwapScheduled);

    const swapRequestId = swapRequestedEvent.swapRequestId;
    await cf.stepUntilEvent(
      'Swapping.SwapRequestCompleted',
      swappingSwapRequestCompleted.refine((event) => event.swapRequestId === swapRequestId),
    );
    swapContext?.updateStatus(cf.logger, SwapStatus.SwapCompleted);

    cf.debug(
      `Swap Request Completed. Waiting for egress scheduled event, balance increase and CCM emitted if CCM swap.`,
    );

    await waitForEgressScheduled(cf, swapRequestId, swapContext);

    const [newBalance] = await Promise.all([
      observeBalanceIncrease(cf.logger, destAsset, destAddress, oldBalance),
      ccmEventEmitted,
    ]);
    cf.debug(`Swap success!${newBalance !== undefined ? ` New balance: ${newBalance}` : ''}!`);

    if (sourceAsset === 'Sol') {
      // Native Vault swaps are fetched proactively. SPL-tokens don't need a fetch.
      const swapEndpointNativeVaultAddress = getContractAddress(
        'Solana',
        'SWAP_ENDPOINT_NATIVE_VAULT_ACCOUNT',
      );
      cf.trace(
        `$Waiting for Swap Endpoint Native Vault Swap Fetch ${swapEndpointNativeVaultAddress}`,
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
      cf.trace(err.stack ?? '');
    }
    return throwError(cf.logger, new Error(`${err}`));
  }
}

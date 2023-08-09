import { encodeAddress } from '@polkadot/util-crypto';
import { Asset } from '@chainflip-io/cli';
import { newSwap } from './new_swap';
import { send, sendViaCfTester } from './send';
import { getBalance } from './get_balance';
import {
  getChainflipApi,
  observeBalanceIncrease,
  observeEvent,
  observeCcmReceived,
  assetToChain,
} from '../shared/utils';
import { CcmDepositMetadata } from '../shared/new_swap';

function encodeDestinationAddress(address: string, destAsset: Asset): string {
  let destAddress = address;

  if (destAddress && destAsset === 'DOT') {
    destAddress = encodeAddress(destAddress);
  }

  return destAddress;
}

export type SwapParams = {
  sourceAsset: Asset;
  destAsset: Asset;
  depositAddress: string;
  destAddress: string;
  channelId: number;
};

export async function requestNewSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  tag = '',
  messageMetadata?: CcmDepositMetadata,
): Promise<SwapParams> {
  const chainflipApi = await getChainflipApi();

  const addressPromise = observeEvent(
    'swapping:SwapDepositAddressReady',
    chainflipApi,

    (event) => {
      // Find deposit address for the right swap by looking at destination address:
      const destAddressEvent = encodeDestinationAddress(
        event.data.destinationAddress[assetToChain(destAsset)],
        destAsset,
      );
      if (!destAddressEvent) return false;

      const destAssetMatches = event.data.destinationAsset.toUpperCase() === destAsset;
      const sourceAssetMatches = event.data.sourceAsset.toUpperCase() === sourceAsset;
      const destAddressMatches =
        destAddressEvent.toLowerCase() ===
        encodeDestinationAddress(destAddress, destAsset).toLowerCase();

      return destAddressMatches && destAssetMatches && sourceAssetMatches;
    },
  );
  await newSwap(sourceAsset, destAsset, destAddress, messageMetadata);

  const res = (await addressPromise).data;

  const depositAddress = res.depositAddress[assetToChain(sourceAsset)];
  const channelDestAddress = res.destinationAddress[assetToChain(destAsset)];
  const channelId = Number(res.channelId);

  console.log(`${tag} Swap address: ${depositAddress}`);
  console.log(`${tag} Destination address is: ${channelDestAddress} Channel ID is: ${channelId}`);

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
  Contract,
}

export async function doPerformSwap(
  { sourceAsset, destAsset, destAddress, depositAddress, channelId }: SwapParams,
  tag = '',
  messageMetadata?: CcmDepositMetadata,
  senderType = SenderType.Address,
) {
  const chainflipApi = await getChainflipApi();

  const oldBalance = await getBalance(destAsset, destAddress);

  console.log(`${tag} Old balance: ${oldBalance}`);

  const swapScheduledHandle = observeEvent('swapping:SwapScheduled', chainflipApi, (event) => {
    if ('DepositChannel' in event.data.origin) {
      const channelMatches = Number(event.data.origin.DepositChannel.channelId) === channelId;
      const assetMatches = sourceAsset === (event.data.sourceAsset.toUpperCase() as Asset);
      return channelMatches && assetMatches;
    }
    // Otherwise it was a swap scheduled by interacting with the ETH smart contract
    return false;
  });

  const ccmEventEmitted = messageMetadata
    ? observeCcmReceived(sourceAsset, destAsset, destAddress, messageMetadata)
    : Promise.resolve();

  await (senderType === SenderType.Address
    ? send(sourceAsset, depositAddress)
    : sendViaCfTester(sourceAsset, depositAddress));

  console.log(`${tag} Funded the address`);

  await swapScheduledHandle;

  console.log(`${tag} Waiting for balance to update`);

  try {
    const [newBalance] = await Promise.all([
      observeBalanceIncrease(destAsset, destAddress, oldBalance),
      ccmEventEmitted,
    ]);

    console.log(`${tag} Swap success! New balance: ${newBalance}!`);
  } catch (err) {
    throw new Error(`${tag} ${err}`);
  }
}

export async function performSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  swapTag?: string,
  messageMetadata?: CcmDepositMetadata,
  senderType = SenderType.Address,
) {
  const tag = swapTag ?? '';

  console.log(
    `${tag} The args are:  ${sourceAsset} ${destAsset} ${destAddress} ${
      messageMetadata ? `someMessage` : ''
    }`,
  );

  const swapParams = await requestNewSwap(
    sourceAsset,
    destAsset,
    destAddress,
    tag,
    messageMetadata,
  );
  await doPerformSwap(swapParams, tag, messageMetadata, senderType);

  return swapParams;
}

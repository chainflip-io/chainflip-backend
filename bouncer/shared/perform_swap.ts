import { encodeAddress } from '@polkadot/util-crypto';
import { Asset } from '@chainflip-io/cli';
import { newSwap } from './new_swap';
import { send } from './send';
import { getBalance } from './get_balance';
import {
  getChainflipApi,
  observeBalanceIncrease,
  observeEvent,
  observeCcmReceived,
  encodeBtcAddressForContract,
} from '../shared/utils';
import { CcmDepositMetadata } from '../shared/new_swap';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function extractDestinationAddress(swapInfo: any, destAsset: Asset): string | undefined {
  const asset = destAsset === 'USDC' || destAsset === 'FLIP' ? 'ETH' : destAsset;
  return swapInfo[1][asset.toLowerCase()];
}

function encodeDestinationAddress(address: string, destAsset: Asset): string {
  let destAddress = address;

  if (destAddress && destAsset === 'BTC') {
    destAddress = destAddress.replace(/^0x/, '');
    destAddress = Buffer.from(destAddress, 'hex').toString();
  }
  if (destAddress && destAsset === 'DOT') {
    destAddress = encodeAddress(destAddress);
  }

  return destAddress;
}

export async function performSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  swapTag?: string,
  messageMetadata?: CcmDepositMetadata,
) {
  const FEE = 100;

  const tag = swapTag ?? '';

  const chainflipApi = await getChainflipApi();

  const addressPromise = observeEvent(
    'swapping:SwapDepositAddressReady',
    chainflipApi,
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (swapInfo: any) => {
      // Find deposit address for the right swap by looking at destination address:
      const destAddressEvent = extractDestinationAddress(swapInfo, destAsset);
      if (!destAddressEvent) return false;

      const destAddressEncoded = encodeDestinationAddress(destAddressEvent, destAsset);

      const destAssetMatches =
        swapInfo[4].charAt(0) + swapInfo[4].slice(1).toUpperCase() === destAsset;
      const sourceAssetMatches =
        swapInfo[3].charAt(0) + swapInfo[3].slice(1).toUpperCase() === sourceAsset;
      const destAddressMatches = destAddressEncoded.toLowerCase() === destAddress.toLowerCase();

      return destAddressMatches && destAssetMatches && sourceAssetMatches;
    },
  );

  await newSwap(sourceAsset, destAsset, destAddress, FEE, messageMetadata);

  console.log(
    `${tag} The args are:  ${sourceAsset} ${destAsset} ${destAddress} ${FEE} ${
      messageMetadata ? `someMessage` : ''
    }`,
  );

  let depositAddressAsset = sourceAsset;
  if (sourceAsset === 'USDC' || sourceAsset === 'FLIP') {
    depositAddressAsset = 'ETH';
  }

  const swapInfo = JSON.parse((await addressPromise).toString());
  let depositAddress = swapInfo[0][depositAddressAsset.toLowerCase()];
  const channelDestAddress = extractDestinationAddress(swapInfo, destAsset);
  const channelId = Number(swapInfo[5]);

  console.log(`${tag} Destination address is: ${channelDestAddress} Channel ID is: ${channelId}`);

  if (sourceAsset === 'BTC') {
    depositAddress = encodeBtcAddressForContract(depositAddress);
  }

  console.log(`${tag} Swap address: ${depositAddress}`);

  const OLD_BALANCE = await getBalance(destAsset, destAddress);

  console.log(`${tag} Old balance: ${OLD_BALANCE}`);

  const swapScheduledHandle = observeEvent('swapping:SwapScheduled', chainflipApi, (event) => {
    if ('depositChannel' in event[5]) {
      const channelMatches = Number(event[5].depositChannel.channelId) === channelId;
      const assetMatches = sourceAsset === (event[1].toUpperCase() as Asset);
      return channelMatches && assetMatches;
    }
    // Otherwise it was a swap scheduled by interacting with the ETH smart contract
    return false;
  });

  const ccmEventEmitted = messageMetadata
    ? observeCcmReceived(sourceAsset, destAsset, destAddress, messageMetadata)
    : Promise.resolve();

  await send(sourceAsset, depositAddress.toLowerCase());
  console.log(`${tag} Funded the address`);

  await swapScheduledHandle;

  console.log(`${tag} Waiting for balance to update`);

  try {
    const [newBalance] = await Promise.all([
      observeBalanceIncrease(destAsset, destAddress, OLD_BALANCE),
      ccmEventEmitted,
    ]);

    console.log(`${tag} Swap success! New balance: ${newBalance}!`);
  } catch (err) {
    throw new Error(`${tag} ${err}`);
  }
}

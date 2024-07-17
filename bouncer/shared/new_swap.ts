import { InternalAsset as Asset, Chain, Asset as SCAsset, broker } from '@chainflip/cli';
import { decodeDotAddressForContract, chainFromAsset, stateChainAssetFromAsset } from './utils';

const defaultCommissionBps = 100; // 1%

export interface CcmDepositMetadata {
  message: string;
  gasBudget: number;
  cfParameters: string;
}

export async function newSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  messageMetadata?: CcmDepositMetadata,
  brokerCommissionBps = defaultCommissionBps,
  boostFeeBps = 0,
): Promise<void> {
  const destinationAddress =
    destAsset === 'Dot' ? decodeDotAddressForContract(destAddress) : destAddress;
  const brokerUrl = process.env.BROKER_ENDPOINT || 'http://127.0.0.1:10997';

  // If the dry_run of the extrinsic fails on the broker-api then it won't retry. So we retry here to
  // avoid flakiness on CI.
  let retryCount = 0;
  while (retryCount < 20) {
    try {
      await broker.requestSwapDepositAddress(
        {
          srcAsset: stateChainAssetFromAsset(sourceAsset) as SCAsset,
          destAsset: stateChainAssetFromAsset(destAsset) as SCAsset,
          srcChain: chainFromAsset(sourceAsset) as Chain,
          destAddress: destinationAddress,
          destChain: chainFromAsset(destAsset) as Chain,
          ccmMetadata: messageMetadata && {
            message: messageMetadata.message as `0x${string}`,
            gasBudget: messageMetadata.gasBudget.toString(),
          },
          maxBoostFeeBps: boostFeeBps,
          refundParameters: {
            retryDuration: 500,
            refundAddress: '0xa56A6be23b6Cf39D9448FF6e897C29c41c8fbDFF',
            minPrice: '100',
          },
        },
        {
          url: brokerUrl,
          commissionBps: brokerCommissionBps,
        },
        'backspin',
      );
      break; // Exit the loop on success
    } catch (error) {
      retryCount++;
      console.error(
        `Request swap deposit address for ${sourceAsset} attempt: ${retryCount} failed: ${error}`,
      );
    }
  }
}

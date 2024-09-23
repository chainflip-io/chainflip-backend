import { InternalAsset as Asset, Chain, Asset as SCAsset, broker } from '@chainflip/cli';
import { decodeDotAddressForContract, chainFromAsset, stateChainAssetFromAsset } from './utils';

const defaultCommissionBps = 100; // 1%

export interface CcmDepositMetadata {
  message: string;
  gasBudget: number;
  cfParameters: string;
}

export interface FillOrKillParamsX128 {
  retryDurationBlocks: number;
  refundAddress: string;
  minPriceX128: string;
}

export interface DcaParams {
  numberOfChunks: number;
  chunkInterval: number;
}

export async function newSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  messageMetadata?: CcmDepositMetadata,
  brokerCommissionBps = defaultCommissionBps,
  boostFeeBps = 0,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
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
          ccmParams: messageMetadata && {
            message: messageMetadata.message as `0x${string}`,
            gasBudget: messageMetadata.gasBudget.toString(),
            cfParameters: messageMetadata.cfParameters as `0x${string}`,
          },
          commissionBps: brokerCommissionBps,
          maxBoostFeeBps: boostFeeBps,
          fillOrKillParams,
          dcaParams,
        },
        {
          url: brokerUrl,
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

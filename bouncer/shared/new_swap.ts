import { InternalAsset as Asset, Chain, Asset as SCAsset, broker } from '@chainflip/cli';
import {
  decodeDotAddressForContract,
  chainFromAsset,
  stateChainAssetFromAsset,
  isPolkadotAsset,
} from './utils';

const defaultCommissionBps = 100; // 1%

type RequestDepositChannelParams = Parameters<(typeof broker)['requestSwapDepositAddress']>[0];

export type CcmDepositMetadata = NonNullable<RequestDepositChannelParams['ccmParams']>;
export type FillOrKillParamsX128 = NonNullable<RequestDepositChannelParams['fillOrKillParams']>;
export type DcaParams = NonNullable<RequestDepositChannelParams['dcaParams']>;

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
  const destinationAddress = isPolkadotAsset(destAsset)
    ? decodeDotAddressForContract(destAddress)
    : destAddress;
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
            ccmAdditionalData: messageMetadata.ccmAdditionalData as `0x${string}`,
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

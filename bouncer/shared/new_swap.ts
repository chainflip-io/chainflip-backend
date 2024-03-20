import { InternalAsset as Asset, Asset as SCAsset, broker, assetConstants } from '@chainflip/cli';
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
): Promise<void> {
  const destinationAddress =
    destAsset === 'Dot' ? decodeDotAddressForContract(destAddress) : destAddress;
  const brokerUrl = process.env.BROKER_ENDPOINT || 'http://127.0.0.1:10997';

  await broker.requestSwapDepositAddress(
    {
      // TODO: Temporal workaround: To remove once SDK supports Arbitrum
      srcAsset: (assetConstants[sourceAsset]
        ? stateChainAssetFromAsset(sourceAsset)
        : sourceAsset.toUpperCase().replace('ARB', '')) as SCAsset,
      // TODO: Temporal workaround: To remove once SDK supports Arbitrum
      destAsset: assetConstants[destAsset]
        ? assetConstants[destAsset].asset
        : (destAsset.toUpperCase().replace('ARB', '') as SCAsset),
      srcChain: chainFromAsset(sourceAsset),
      destAddress: destinationAddress,
      destChain: chainFromAsset(destAsset),
      ccmMetadata: messageMetadata && {
        message: messageMetadata.message as `0x${string}`,
        gasBudget: messageMetadata.gasBudget.toString(),
      },
    },
    {
      url: brokerUrl,
      commissionBps: brokerCommissionBps,
    },
    'backspin',
  );
}

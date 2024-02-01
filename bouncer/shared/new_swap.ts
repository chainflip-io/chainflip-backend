import { Asset, broker } from '@chainflip-io/cli';
import { decodeDotAddressForContract, chainFromAsset } from './utils';

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
    destAsset === 'DOT' ? decodeDotAddressForContract(destAddress) : destAddress;
  const brokerUrl = process.env.BROKER_ENDPOINT || 'http://127.0.0.1:10997';
  await broker.requestSwapDepositAddress(
    {
      srcAsset: sourceAsset,
      destAsset,
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
  );
}

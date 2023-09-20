import { Asset, BrokerClient } from '@chainflip-io/cli';
import { decodeDotAddressForContract, chainFromAsset } from './utils';

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
): Promise<void> {
  const destinationAddress =
    destAsset === 'DOT' ? decodeDotAddressForContract(destAddress) : destAddress;

  const client = await BrokerClient.create({
    url: process.env.BROKER_ENDPOINT ?? 'ws://127.0.0.1:10997',
  });

  await client.requestSwapDepositAddress({
    srcAsset: sourceAsset,
    destAsset,
    srcChain: chainFromAsset(sourceAsset),
    destAddress: destinationAddress,
    destChain: chainFromAsset(destAsset),
    ccmMetadata: messageMetadata && {
      message: messageMetadata.message as `0x${string}`,
      gasBudget: messageMetadata.gasBudget.toString(),
    },
  });
}

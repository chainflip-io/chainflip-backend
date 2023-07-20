import { cryptoWaitReady } from '@polkadot/util-crypto';
import { Asset, BrokerClient } from '@chainflip-io/cli';
import { encodeDotAddressForContract, chainFromAsset } from './utils';

export interface CcmDepositMetadata {
  message: string;
  gasBudget: number;
  cfParameters: string;
}

export async function newSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  fee: number,
  messageMetadata?: CcmDepositMetadata,
): Promise<void> {
  await cryptoWaitReady();

  const destinationAddress =
    destAsset === 'DOT' ? encodeDotAddressForContract(destAddress) : destAddress;

  const client = await BrokerClient.create({
    url: process.env.BROKER_ENDPOINT ?? 'ws://127.0.0.1:10997',
  });

  await client.requestSwapDepositAddress({
    srcAsset: sourceAsset,
    destAsset,
    srcChain: chainFromAsset(sourceAsset),
    destAddress: destinationAddress,
    destChain: chainFromAsset(destAsset),
    ccmMetadata: messageMetadata,
  });
}
